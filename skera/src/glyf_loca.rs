//! impl subset() for glyf and loca
use std::collections::HashSet;

use fnv::FnvHashMap;
use log::debug;

use crate::{
    estimate_subset_table_size,
    head::HeadMaxpInfo,
    serialize::{SerializeErrorFlags, Serializer},
    Plan, Subset,
    SubsetError::{self, SubsetTableError},
    SubsetFlags,
};
use font_types::{F2Dot14, Point};
use skrifa::{
    prelude::{LocationRef, Size},
    raw::{tables::glyf::CurvePoint, ReadError},
    MetadataProvider,
};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{
        tables::{
            glyf::{
                Anchor, CompositeGlyph, CompositeGlyphFlags, Glyf,
                Glyph::{self, Composite, Simple},
                SimpleGlyph, SimpleGlyphFlags,
            },
            head::Head,
            loca::Loca,
        },
        types::GlyphId,
        FontRef, TableProvider, TopLevelTable,
    },
    tables::glyf::{Bbox, CompositeGlyph as WriteCompositeGlyph},
    FontBuilder, OtRound,
};

pub(crate) const PHANTOM_POINT_COUNT: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Bounds {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
}

impl Bounds {
    fn is_empty(&self) -> bool {
        self.x_min == 0.0 && self.y_min == 0.0 && self.x_max == 0.0 && self.y_max == 0.0
    }
}

impl Into<write_fonts::tables::glyf::Bbox> for Bounds {
    fn into(self) -> write_fonts::tables::glyf::Bbox {
        write_fonts::tables::glyf::Bbox {
            x_min: self.x_min.round() as i16,
            y_min: self.y_min.round() as i16,
            x_max: self.x_max.round() as i16,
            y_max: self.y_max.round() as i16,
        }
    }
}

// reference: subset() for glyf/loca/head in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/OT/glyf/glyf.hh#L77
impl Subset for Glyf<'_> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let loca = font.loca(None).or(Err(SubsetTableError(Loca::TAG)))?;
        let head = font.head().or(Err(SubsetTableError(Head::TAG)))?;

        let num_output_glyphs = plan.num_output_glyphs;
        let mut subset_glyphs = Vec::with_capacity(num_output_glyphs);
        let mut max_offset: u32 = 0;

        let glyf_accelerator = GlyfAccelerator::new(font, plan);

        // _populate_subset_glyphs
        for (new_gid, old_gid) in &plan.new_to_old_gid_list {
            match loca.get_glyf(*old_gid, self) {
                Ok(g) => {
                    let Some(glyph) = g else {
                        subset_glyphs.push(Vec::new());
                        continue;
                    };

                    if *old_gid == GlyphId::NOTDEF
                        && *new_gid == GlyphId::NOTDEF
                        && !plan
                            .subset_flags
                            .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
                    {
                        subset_glyphs.push(Vec::new());
                        continue;
                    }

                    let subset_glyph = if !plan.normalized_coords.is_empty() {
                        // I *presume* this is old_gid since we are pretending to be the old font when applying deltas, but this is a bit awkward and should be cleaned up.
                        compile_bytes_with_deltas(glyph, plan, &glyf_accelerator, *old_gid)
                            .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?
                    } else {
                        subset_glyph(&glyph, plan)
                    };

                    let trimmed_len = subset_glyph.len();
                    max_offset += padded_size(trimmed_len) as u32;
                    subset_glyphs.push(subset_glyph);
                }
                _ => {
                    return Err(SubsetTableError(Glyf::TAG));
                }
            }
        }

        let loca_format: u8 = if max_offset < 0x1FFFF
            && !plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_FORCE_LONG_LOCA)
        {
            0
        } else {
            1
        };
        let loca_out = write_glyf_loca(font, plan, s, loca_format, &subset_glyphs)?;

        let head_out = crate::head::subset_head(&head, loca_format, plan);

        builder.add_raw(Loca::TAG, loca_out);
        builder.add_raw(Head::TAG, head_out);
        Ok(())
    }
}

fn padded_size(len: usize) -> usize {
    len + len % 2
}

// glyf data is written into the serializer, returning loca data to be added by FontBuilder
fn write_glyf_loca(
    font: &FontRef,
    plan: &Plan,
    s: &mut Serializer,
    loca_format: u8,
    subset_glyphs: &[Vec<u8>],
) -> Result<Vec<u8>, SubsetError> {
    let loca_cap = estimate_subset_table_size(font, Loca::TAG, plan);
    let mut loca_out: Vec<u8> = Vec::with_capacity(loca_cap);

    if loca_format == 0 {
        loca_out.extend_from_slice(&0_u16.to_be_bytes());
    } else {
        loca_out.extend_from_slice(&0_u32.to_be_bytes());
    }

    let init_len = s.length();
    let mut last: u32 = 0;
    if loca_format == 0 {
        let mut offset: u32 = 0;
        let mut value = 0_u16.to_be_bytes();
        for ((new_gid, _), i) in plan.new_to_old_gid_list.iter().zip(0u16..) {
            let gid = new_gid.to_u32();

            while last < gid {
                loca_out.extend_from_slice(&value);
                last += 1;
            }
            let g = &subset_glyphs[i as usize];
            let padded_len = padded_size(g.len());
            offset += padded_len as u32;
            value = ((offset >> 1) as u16).to_be_bytes();
            loca_out.extend_from_slice(&value);
            s.embed_bytes(g)
                .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?;
            if padded_len > g.len() {
                s.embed_bytes(&[0])
                    .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?;
            }

            last += 1;
        }

        while last < plan.num_output_glyphs as u32 {
            loca_out.extend_from_slice(&value);
            last += 1;
        }
    } else {
        let mut offset: u32 = 0;
        let mut value = 0_u32.to_be_bytes();
        for ((new_gid, _), i) in plan.new_to_old_gid_list.iter().zip(0u16..) {
            let gid = new_gid.to_u32();

            while last < gid {
                loca_out.extend_from_slice(&value);
                last += 1;
            }
            let g = &subset_glyphs[i as usize];
            offset += g.len() as u32;
            value = offset.to_be_bytes();
            loca_out.extend_from_slice(&value);

            s.embed_bytes(g)
                .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?;

            last += 1;
        }

        while last < plan.num_output_glyphs as u32 {
            loca_out.extend_from_slice(&value);
            last += 1;
        }
    }

    // As a special case when all glyph in the font are empty, add a zero byte to the table,
    // so that OTS doesn’t reject it, and to make the table work on Windows as well.
    // See https://github.com/khaledhosny/ots/issues/52
    if init_len == s.length() {
        s.embed_bytes(&[0])
            .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?;
    }

    Ok(loca_out)
}

fn subset_glyph(glyph: &Glyph, plan: &Plan) -> Vec<u8> {
    match glyph {
        Composite(comp_g) => subset_composite_glyph(comp_g, plan),
        Simple(simple_g) => subset_simple_glyph(simple_g, plan),
    }
}

fn subset_simple_glyph(g: &SimpleGlyph, plan: &Plan) -> Vec<u8> {
    let mut out = Vec::with_capacity(g.offset_data().len());

    let Some(num_coords) = g.end_pts_of_contours().last() else {
        return out;
    };
    let num_coords = num_coords.get() + 1;
    let glyph_data = g.glyph_data();
    let i = trim_simple_glyph_padding(glyph_data, num_coords);
    if i == 0 {
        return out;
    }

    let glyph_bytes = g.offset_data().as_bytes();
    let header_len = 10 + 2 * (g.number_of_contours() as usize) + 2;
    let Some(header_slice) = glyph_bytes.get(0..header_len) else {
        return out;
    };
    out.extend_from_slice(header_slice);

    if plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
    {
        // drop hints: set instructionLength field to 0
        out[header_len - 2] = 0;
        out[header_len - 1] = 0;
    } else {
        let instruction_end = header_len + g.instruction_length() as usize;
        let Some(instruction_slice) = glyph_bytes.get(header_len..instruction_end) else {
            return Vec::new();
        };
        out.extend_from_slice(instruction_slice);
    }

    let Some(trimmed_slice) = glyph_data.get(0..i) else {
        return Vec::new();
    };
    let first_flag_index = out.len();
    out.extend_from_slice(trimmed_slice);
    if plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG)
    {
        out[first_flag_index] |= SimpleGlyphFlags::OVERLAP_SIMPLE.bits();
    }
    out
}

fn subset_composite_glyph(g: &CompositeGlyph, plan: &Plan) -> Vec<u8> {
    let mut out = g.offset_data().as_bytes().to_owned();

    let mut more = true;
    let mut we_have_instructions = false;
    let mut i: usize = 10;
    let len: usize = out.len();

    while more {
        if i + 3 >= len {
            return Vec::new();
        }
        let flags = u16::from_be_bytes([out[i], out[i + 1]]);
        let mut flags = CompositeGlyphFlags::from_bits_truncate(flags);

        if flags.contains(CompositeGlyphFlags::WE_HAVE_INSTRUCTIONS) {
            we_have_instructions = true;
            if plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
            {
                flags.remove(CompositeGlyphFlags::WE_HAVE_INSTRUCTIONS);
                out.get_mut(i..i + 2)
                    .unwrap()
                    .copy_from_slice(&flags.bits().to_be_bytes());
            }
        }

        // only set overlaps flag on the first component
        if plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG)
            && i == 10
        {
            flags.insert(CompositeGlyphFlags::OVERLAP_COMPOUND);
            out.get_mut(i..i + 2)
                .unwrap()
                .copy_from_slice(&flags.bits().to_be_bytes());
        }

        let old_gid = u16::from_be_bytes([out[i + 2], out[i + 3]]);
        let Some(new_gid) = plan.glyph_map.get(&GlyphId::from(old_gid)) else {
            return Vec::new();
        };
        let new_gid = new_gid.to_u32() as u16;
        out[i + 2] = (new_gid >> 8) as u8;
        out[i + 3] = (new_gid & 0xFF) as u8;

        i += 4;

        if flags.contains(CompositeGlyphFlags::ARG_1_AND_2_ARE_WORDS) {
            i += 4;
        } else {
            i += 2;
        }

        if flags.contains(CompositeGlyphFlags::WE_HAVE_A_SCALE) {
            i += 2;
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE) {
            i += 4;
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO) {
            i += 8;
        }

        more = flags.contains(CompositeGlyphFlags::MORE_COMPONENTS);
    }

    if we_have_instructions
        && !plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
    {
        if i + 1 >= len {
            return Vec::new();
        }
        let instruction_len = u16::from_be_bytes([out[i], out[i + 1]]);
        i += 2 + instruction_len as usize;
    }

    out.truncate(i);
    out
}

// trim padding bytes for simple glyphs, return trimmed length of the raw data for flags & x/y coordinates
fn trim_simple_glyph_padding(glyph_data: &[u8], num_coords: u16) -> usize {
    let mut coord_bytes: usize = 0;
    let mut coords_with_flags: u16 = 0;
    let length = glyph_data.len();
    let mut i: usize = 0;
    while i < length {
        let flag = SimpleGlyphFlags::from_bits_truncate(glyph_data[i]);
        i += 1;

        let mut repeat: u8 = 1;
        if flag.contains(SimpleGlyphFlags::REPEAT_FLAG) {
            if i >= length {
                return 0;
            }
            repeat = glyph_data[i] + 1;
            i += 1;
        }

        let mut x_bytes: u8 = 0;
        let mut y_bytes: u8 = 0;
        if flag.contains(SimpleGlyphFlags::X_SHORT_VECTOR) {
            x_bytes = 1;
        } else if !flag.contains(SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR) {
            x_bytes = 2;
        }

        if flag.contains(SimpleGlyphFlags::Y_SHORT_VECTOR) {
            y_bytes = 1;
        } else if !flag.contains(SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR) {
            y_bytes = 2;
        }

        coord_bytes += ((x_bytes + y_bytes) * repeat) as usize;
        coords_with_flags += repeat as u16;
        if coords_with_flags >= num_coords {
            break;
        }
    }

    if num_coords != coords_with_flags {
        return 0;
    }
    i += coord_bytes;
    i
}

struct GlyfAccelerator<'a> {
    loca: Loca<'a>,
    head: Head<'a>,
    instance_deltas: &'a FnvHashMap<GlyphId, Vec<Point<f32>>>, // *new* GID deltas
    hmtx: skrifa::raw::tables::hmtx::Hmtx<'a>,
    vmtx: Option<skrifa::raw::tables::vmtx::Vmtx<'a>>,
    glyf: Glyf<'a>,
}

impl<'a> GlyfAccelerator<'a> {
    fn new(font: &'a FontRef, plan: &'a Plan) -> GlyfAccelerator<'a> {
        let loca = font
            .loca(None)
            .expect("glyf/loca tables are required for subsetting");
        let head = font.head().expect("head table is required for subsetting");
        let hmtx = font.hmtx().expect("hmtx table is required for subsetting");
        let vmtx = font.vmtx().ok();
        let glyf = font.glyf().expect("glyf table is required for subsetting");

        Self {
            loca,
            head,
            hmtx,
            vmtx,
            glyf,
            instance_deltas: &plan.new_gid_instance_deltas_map,
        }
    }

    fn left_side_bearing(&self, gid: GlyphId) -> f32 {
        self.hmtx.side_bearing(gid).unwrap_or(0) as f32
    }

    fn advance_width(&self, gid: GlyphId) -> f32 {
        self.hmtx.advance(gid).unwrap_or(0) as f32
    }

    fn get_glyph(&self, gid: GlyphId) -> Option<Glyph> {
        self.loca.get_glyf(gid, &self.glyf).ok()?
    }

    fn units_per_em(&self) -> f32 {
        self.head.units_per_em() as f32
    }

    fn apply_gvar_deltas_to_points(
        &self,
        gid: GlyphId,
        _coords: &[F2Dot14],
        target_points: &mut ContourPoints,
    ) {
        // Harfbuzz has to do this in a generic way, but we only care about deltas at the
        // point of instantiation, which are known and collected in the plan in advance. The
        // Deltas in the plan are keyed by new gid; apply them directly (including phantom deltas).
        if let Some(deltas) = self.instance_deltas.get(&gid) {
            let apply_len = deltas.len().min(target_points.0.len());
            if apply_len == 0 {
                return;
            }
            log::debug!(
                "Applying {} deltas to {} points for gid {}",
                apply_len,
                target_points.0.len(),
                gid
            );
            for (point, delta) in target_points
                .0
                .iter_mut()
                .zip(deltas.iter())
                .take(apply_len)
            {
                point.add_delta(delta.x, delta.y);
            }
        } else {
            log::warn!(
                "No deltas found for gid {}, not applying any gvar deltas",
                gid
            );
        }
    }
}

fn compile_bytes_with_deltas(
    glyph: Glyph,
    plan: &Plan,
    glyph_accelerator: &GlyfAccelerator,
    new_gid: GlyphId,
) -> Result<Vec<u8>, write_fonts::error::Error> {
    let mut write_glyph: write_fonts::tables::glyf::Glyph = glyph.to_owned_table();
    let head_maxp = if matches!(write_glyph, write_fonts::tables::glyf::Glyph::Empty)
        || (new_gid == GlyphId::NOTDEF
            && !plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE))
    {
        None
    } else {
        plan.head_maxp_info.try_borrow_mut().ok()
    };
    let (all_points, points_with_deltas) =
        get_points(&glyph, plan, glyph_accelerator, new_gid, head_maxp).unwrap();
    log::debug!(
        "Got {} total points and {} points with deltas for gid {}",
        all_points.len(),
        points_with_deltas.as_ref().map(|p| p.len()).unwrap_or(0),
        new_gid
    );
    // .notdef, set type to empty so we only update metrics and don't compile bytes for
    // it
    if new_gid == GlyphId::NOTDEF
        && !plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
    {
        write_glyph = write_fonts::tables::glyf::Glyph::Empty;
    }

    if !plan.pinned_at_default {
        match write_glyph {
            write_fonts::tables::glyf::Glyph::Empty => {}
            write_fonts::tables::glyf::Glyph::Simple(ref mut simple_glyph) => {
                make_simple_glyph_with_deltas(
                    simple_glyph,
                    &all_points, // Not points with deltas, apparently.
                    plan.subset_flags
                        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING),
                );
            }
            write_fonts::tables::glyf::Glyph::Composite(ref mut composite_glyph) => {
                write_glyph =
                    make_composite_glyph_with_deltas(composite_glyph, points_with_deltas, plan);
            }
        }
    }
    compile_header_bytes(&mut write_glyph, plan, all_points, new_gid);
    write_fonts::dump_table(&write_glyph)
}

fn compile_header_bytes(
    write_glyph: &mut write_fonts::tables::glyf::Glyph,
    plan: &Plan,
    all_points: Vec<ContourPoint>,
    new_gid: GlyphId,
) {
    let points = ContourPoints(all_points);
    points.update_mtx(plan, new_gid);
    let Some(points_without_phantoms) = points.without_phantoms() else {
        if let write_fonts::tables::glyf::Glyph::Simple(simple_glyph) = write_glyph {
            simple_glyph.recompute_bounding_box()
        }
        // We *just* have phantoms?
        return;
    };
    let bounds = points_without_phantoms.get_bounds_without_phantoms();
    match write_glyph {
        write_fonts::tables::glyf::Glyph::Empty => {}
        write_fonts::tables::glyf::Glyph::Simple(simple_glyph) => simple_glyph.bbox = bounds.into(),
        write_fonts::tables::glyf::Glyph::Composite(composite_glyph) => {
            log::warn!(
                "Setting bbox for composite glyph {} to {:?} based on points without phantoms",
                new_gid,
                bounds
            );
            composite_glyph.bbox = bounds.into()
        }
    }
}

fn make_composite_glyph_with_deltas(
    composite_glyph: &mut WriteCompositeGlyph,
    points_with_deltas: Option<Vec<ContourPoint>>,
    plan: &Plan,
) -> write_fonts::tables::glyf::Glyph {
    // We can't mutate components, we have to rebuild the glyph
    let Some(points_with_deltas) = points_with_deltas else {
        return write_fonts::tables::glyf::Glyph::Composite(composite_glyph.clone());
    };
    let mut new_components = vec![];
    let component_count = composite_glyph.components().len();
    assert!(points_with_deltas.len() >= component_count + PHANTOM_POINT_COUNT,
        "There should be at least as many points with deltas ({}) as there are components plus the phantom points ({})",
        points_with_deltas.len(),
        component_count + PHANTOM_POINT_COUNT,
    );
    let points_without_phantoms: Vec<ContourPoint> = points_with_deltas
        .into_iter()
        .take(component_count)
        .collect();

    for (component, transform) in composite_glyph
        .components()
        .iter()
        .zip(points_without_phantoms.iter())
    {
        let mut new_component = component.clone();
        if let Anchor::Offset { .. } = component.anchor {
            new_component.anchor = Anchor::Offset {
                x: transform.x.ot_round(),
                y: transform.y.ot_round(),
            };
        }
        // Harfbuzz creates an intermediate SubsetGlyph which remaps the glyph IDs.
        // We don't have that step, so do it here.
        new_component.glyph = plan
            .new_to_old_gid_list
            .iter()
            .find_map(|(new, old)| {
                if *old == component.glyph {
                    Some(*new)
                } else {
                    None
                }
            })
            .expect("BUG: all component glyphs should have been mapped in Plan::new()")
            .try_into()
            .unwrap();
        // XXX I'm not entirely sure what deltas are generated for other uses
        new_components.push(new_component);
    }
    let first = new_components.remove(0);
    let mut new_composite = WriteCompositeGlyph::new(first, composite_glyph.bbox);
    for component in new_components {
        new_composite.add_component(component, composite_glyph.bbox);
    }
    // We fix up the header stuff later
    write_fonts::tables::glyf::Glyph::Composite(new_composite)
}

/// Wrapper around get_points_harfbuzz_standalone that returns point data in the format expected by subsetting code
/// Returns: (all_points, ContourPoints with deltas, composite_contours_count)
fn get_points(
    read_glyph: &Glyph,
    plan: &Plan,
    glyph_accelerator: &GlyfAccelerator,
    gid: GlyphId,
    mut head_maxp_opt: Option<std::cell::RefMut<HeadMaxpInfo>>,
) -> Result<(Vec<ContourPoint>, Option<Vec<ContourPoint>>), SerializeErrorFlags> {
    // TODO: Implement wrapper that:
    // 1. Looks up original glyph from loca/glyf
    // 2. Gets FontRef from plan
    // 3. Converts write_glyph back to read form
    // 4. Calls get_points_harfbuzz_standalone with normalized coords from plan
    // 5. Returns results in the expected format

    // For now, return empty results as placeholder
    let mut comp_points_scratch: Vec<ContourPoint> = Vec::new();
    let mut composite_contours: usize = 0;
    get_points_harfbuzz_standalone(
        Some(read_glyph),
        gid,
        glyph_accelerator,
        &mut head_maxp_opt,
        &mut comp_points_scratch,
        &plan.normalized_coords,
        false,
        0,
        &mut HashSet::new(),
        &mut composite_contours,
    )
}

fn make_simple_glyph_with_deltas(
    simple_glyph: &mut write_fonts::tables::glyf::SimpleGlyph,
    points_with_deltas: &[ContourPoint],
    no_hinting: bool,
) {
    if no_hinting {
        simple_glyph.instructions = vec![];
    }
    simple_glyph.contours = vec![];
    let mut last_contour: Vec<CurvePoint> = vec![];
    let mut x_min: i16 = 0;
    let mut y_min: i16 = 0;
    let mut x_max: i16 = 0;
    let mut y_max: i16 = 0;
    // unsigned num_points = all_points.length - 4; ->
    // last 4 points in points_with_deltas are phantom points and should not be included
    for (ix, point) in points_with_deltas.iter().enumerate() {
        if ix >= points_with_deltas.len() - 4 {
            break;
        }
        last_contour.push(CurvePoint {
            x: point.x.ot_round(),
            y: point.y.ot_round(),
            on_curve: point.is_on_curve,
        });
        x_min = x_min.min(point.x.ot_round());
        y_min = y_min.min(point.y.ot_round());
        x_max = x_max.max(point.x.ot_round());
        y_max = y_max.max(point.y.ot_round());
        if point.is_end_point {
            simple_glyph.contours.push(last_contour.into());
            last_contour = vec![];
        }
    }
}

// Pieces of the compile_header_bytes function. Will put them together later.
//
//     match write_glyph {
//         write_fonts::tables::glyf::Glyph::Empty => {}
//         write_fonts::tables::glyf::Glyph::Simple(ref mut simple_glyph) => {
//             if plan
//                 .subset_flags
//                 .contains(SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG)
//             {
//                 // Oops, write_fonts doesn't let us do this
//                 // simple_glyph.flags.insert(SimpleGlyphFlags::OVERLAP_SIMPLE);
//             }
//             simple_glyph.recompute_bounding_box();
//         }
//         write_fonts::tables::glyf::Glyph::Composite(ref mut composite_glyph) => {
//
//             }
//             if new_components.is_empty() {
//                 // Not sure how this can happen but I don't really want to panic either
//                 return Ok(vec![]);
//             }
//             let mut first = new_components.remove(0);
//             if plan
//                 .subset_flags
//                 .contains(SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG)
//             {
//                 first.flags.overlap_compound = true;
//             }
//             let mut new_composite = WriteCompositeGlyph::new(first, composite_glyph.bbox);
//             for component in new_components {
//                 new_composite.add_component(component, composite_glyph.bbox);
//             }
//             if !plan
//                 .subset_flags
//                 .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
//             {
//                 new_composite.add_instructions(composite_glyph.instructions());
//             }
//             *composite_glyph = new_composite;
//         }
//     }

//     write_fonts::dump_table(&write_glyph)
// }

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContourPoint {
    pub x: f32,
    pub y: f32,
    pub is_end_point: bool,
    pub is_on_curve: bool,
}
impl ContourPoint {
    fn new(x: f32, y: f32, is_on_curve: bool, is_end_point: bool) -> Self {
        Self {
            x,
            y,
            is_end_point,
            is_on_curve,
        }
    }
    fn add_delta(&mut self, delta_x: f32, delta_y: f32) {
        self.x += delta_x;
        self.y += delta_y;
    }
}

impl From<CurvePoint> for ContourPoint {
    fn from(curve_point: CurvePoint) -> Self {
        Self {
            x: curve_point.x as f32,
            y: curve_point.y as f32,
            is_end_point: false,
            is_on_curve: curve_point.on_curve,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContourPoints(pub Vec<ContourPoint>);
impl ContourPoints {
    fn new() -> Self {
        Self(Vec::new())
    }
    pub(crate) fn add_deltas(&mut self, deltas: &[Point<f32>]) {
        for (i, delta) in deltas.iter().enumerate() {
            self.0[i].add_delta(delta.x, delta.y);
        }
    }
    pub(crate) fn add_deltas_with_indices(
        &mut self,
        deltas_x: &[f32],
        deltas_y: &[f32],
        indices: &[bool],
    ) {
        for i in 0..deltas_x.len() {
            if indices[i] {
                self.0[i].add_delta(deltas_x[i], deltas_y[i]);
            }
        }
    }

    fn without_phantoms(&self) -> Option<ContourPoints> {
        if self.0.len() < PHANTOM_POINT_COUNT {
            return None;
        }
        Some(ContourPoints(
            self.0
                .iter()
                .take(self.0.len() - PHANTOM_POINT_COUNT)
                .cloned()
                .collect(),
        ))
    }
}

#[derive(Debug, Clone, Default)]
struct ContourPointState {
    depth: usize,
    decycler: HashSet<GlyphId>,
    composite_contours: usize,
    // Whether we're applying gvar deltas during instantiation
    apply_deltas: bool,
    // Pre-computed deltas for components from the plan
    deltas_for_component: Option<Point<f32>>,
}

impl ContourPoints {
    /// Recursively gather contour points from a glyph and its components.
    /// This faithfully follows the Harfbuzz algorithm in Glyph::get_points().
    /// For simple glyphs: collects the contour points directly.
    /// For composite glyphs: recursively gathers points from all components.
    /// For empty glyphs: returns an empty vector.
    fn get_points_and_metrics(
        glyph: &Glyph,
        gid: GlyphId,
        font: &FontRef,
        head_maxp_info: &mut HeadMaxpInfo,
        state: &mut ContourPointState,
    ) -> Result<(Vec<ContourPoint>, Option<GlyphId>, f32, f32), ReadError> {
        let mut steal_metrics = None;
        let mut contour_points = Vec::new();
        head_maxp_info.update_max_component_depth(state.depth);

        match glyph {
            Simple(simple_glyph) => {
                // Collect contour points and mark end points
                let end_points = simple_glyph
                    .end_pts_of_contours()
                    .iter()
                    .map(|e| e.get())
                    .collect::<Vec<u16>>();
                contour_points.extend(simple_glyph.points().enumerate().map(|(ix, p)| {
                    ContourPoint::new(
                        p.x as f32,
                        p.y as f32,
                        p.on_curve,
                        end_points.contains(&(ix as u16)),
                    )
                }));
                for endpoint in simple_glyph.end_pts_of_contours().iter() {
                    contour_points[endpoint.get() as usize].is_end_point = true;
                }
                head_maxp_info.update_max_points(contour_points.len() as u16);
                let num_contours = simple_glyph.number_of_contours() as u16;
                if state.depth == 0 {
                    head_maxp_info.update_max_contours(num_contours);
                } else {
                    state.composite_contours += num_contours as usize;
                }
            }
            Composite(composite_glyph) => {
                if let Some((gid, flags)) = composite_glyph.component_glyphs_and_flags().next() {
                    if flags.contains(CompositeGlyphFlags::USE_MY_METRICS) {
                        steal_metrics = Some(GlyphId::from(gid));
                    }
                }
                for composite in composite_glyph.components() {
                    match composite.anchor {
                        Anchor::Point { .. } => {
                            // if (is_anchored ()) tx = ty = 0;
                            contour_points.push(ContourPoint::new(0.0, 0.0, false, true));
                        }
                        Anchor::Offset { x, y } => {
                            contour_points
                                .push(ContourPoint::new(x as f32, y as f32, false, false));
                        }
                    }
                }
                head_maxp_info.update_max_composite_contours(state.composite_contours as u16);
                head_maxp_info.update_max_composite_points(contour_points.len() as u16);
                head_maxp_info
                    .update_max_component_elements(composite_glyph.components().count() as u16);
            }
        }

        // Get metrics for this glyph
        let (lsb, aw) = if let Some(steal_gid) = steal_metrics {
            // Steal metrics from the first component with USE_MY_METRICS flag
            let loca = font.loca(None)?;
            let glyf = font.glyf()?;
            let other_glyph = loca.get_glyf(steal_gid, &glyf)?;
            let glyph_ref = other_glyph.as_ref().unwrap_or(glyph);

            log::warn!(
                "Recursing to steal metrics from gid {} for gid {}",
                steal_gid,
                gid
            );

            if state.decycler.contains(&steal_gid) {
                let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
                let lsb = metrics.left_side_bearing(steal_gid).unwrap_or(0.0);
                let aw = metrics.advance_width(steal_gid).unwrap_or(0.0);
                return Ok((contour_points, steal_metrics, lsb, aw));
            }

            state.decycler.insert(steal_gid);
            state.depth += 1;

            let (_, _, aw, lsb) =
                Self::get_points_and_metrics(glyph_ref, steal_gid, font, head_maxp_info, state)?;

            state.depth -= 1;
            (lsb, aw)
        } else {
            let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
            let lsb = metrics.left_side_bearing(gid).unwrap_or(0.0);
            let aw = metrics.advance_width(gid).unwrap_or(0.0);
            (lsb, aw)
        };

        Ok((contour_points, steal_metrics, lsb, aw))
    }

    /// Apply component transformation and translation to gathered points.
    /// This follows the Harfbuzz algorithm where component points are transformed
    /// according to the component's anchor (translation) and optional transform matrix.
    fn apply_component_transform(
        points: &mut Vec<ContourPoint>,
        component: &write_fonts::read::tables::glyf::Component,
    ) {
        // Get translation from component anchor
        let (dx, dy) = match component.anchor {
            Anchor::Point { .. } => {
                // Anchor point mode doesn't translate points independently
                (0.0, 0.0)
            }
            Anchor::Offset { x, y } => (x as f32, y as f32),
        };

        // Get scale/rotation from component transformation
        // Default identity transform: [1, 0, 0, 1]
        let transform = component.transform;
        let scale_x = transform.xx.to_f32();
        let scale_y = transform.yy.to_f32();
        let skew_xy = transform.xy.to_f32();
        let skew_yx = transform.yx.to_f32();

        if points.len() > 0 {
            debug!(
                "Applying transform: dx={}, dy={}, scale_x={}, scale_y={}, skew_xy={}, skew_yx={}",
                dx, dy, scale_x, scale_y, skew_xy, skew_yx
            );
            let first_point = &points[0];
            debug!(
                "First point before transform: x={}, y={}",
                first_point.x, first_point.y
            );
        }

        // Apply transformation and translation to each point (including phantom points)
        // Phantom points carry metric information and must be transformed too
        for point in points.iter_mut() {
            let x = point.x;
            let y = point.y;

            // Apply 2x2 matrix transformation: [x', y'] = [[xx, xy], [yx, yy]] * [x, y] + [dx, dy]
            point.x = x * scale_x + y * skew_xy + dx;
            point.y = x * skew_yx + y * scale_y + dy;
        }

        if points.len() > 0 {
            let first_point = &points[0];
            debug!(
                "First point after transform: x={}, y={}",
                first_point.x, first_point.y
            );
        }
    }

    pub(crate) fn get_all_points_without_var(
        glyph: &Option<Glyph<'_>>,
        font: &FontRef<'_>,
        glyph_id: GlyphId,
    ) -> Result<Self, ReadError> {
        let mut points = Vec::new();
        match glyph {
            Some(Glyph::Simple(simple_glyph)) => {
                let end_points = simple_glyph
                    .end_pts_of_contours()
                    .iter()
                    .map(|e| e.get())
                    .collect::<Vec<u16>>();
                points.extend(simple_glyph.points().enumerate().map(|(ix, p)| {
                    ContourPoint::new(
                        p.x as f32,
                        p.y as f32,
                        p.on_curve,
                        end_points.contains(&(ix as u16)),
                    )
                }));
                for endpoint in simple_glyph.end_pts_of_contours().iter() {
                    points[endpoint.get() as usize].is_end_point = true;
                }
            }
            Some(Glyph::Composite(composite_glyph)) => {
                for component in composite_glyph.components() {
                    // The "points" here should be the transformations (CompositeGlyph.hh::get_points)
                    match component.anchor {
                        Anchor::Point { .. } => {
                            // if (is_anchored ()) tx = ty = 0;
                            points.push(ContourPoint::new(0.0, 0.0, false, true));
                        }
                        Anchor::Offset { x, y } => {
                            points.push(ContourPoint::new(x as f32, y as f32, false, false));
                        }
                    }
                }
            }
            None => {}
        };
        let x_min = match glyph {
            Some(Glyph::Simple(simple_glyph)) => simple_glyph.x_min() as f32,
            Some(Glyph::Composite(composite_glyph)) => composite_glyph.x_min() as f32,
            None => 0.0, // empty glyph
        };
        let y_max = match glyph {
            Some(Glyph::Simple(simple_glyph)) => simple_glyph.y_max() as f32,
            Some(Glyph::Composite(composite_glyph)) => composite_glyph.y_max() as f32,
            None => 0.0, // empty glyph
        };
        // Init phantom points
        let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        let h_adv = glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
        let lsb = glyph_metrics.left_side_bearing(glyph_id).unwrap_or(0.0);
        let h_delta = x_min - lsb;
        let tsb = 0.0;
        let v_adv = -(font.head().unwrap().units_per_em() as f32); // XXX use vmtx if available
        let v_orig = y_max + tsb;
        // Phantom left
        points.push(ContourPoint::new(h_delta, 0.0, false, true));
        // Phantom right
        points.push(ContourPoint::new(h_adv + h_delta, 0.0, false, true));
        // Phantom top
        points.push(ContourPoint::new(0.0, v_orig, false, true));
        // Phantom bottom
        points.push(ContourPoint::new(0.0, v_orig - v_adv, false, true));

        Ok(Self(points))
    }

    fn get_bounds(&self) -> Bounds {
        let mut x_min = 0.0;
        let mut x_max = 0.0;
        let mut y_min = 0.0;
        let mut y_max = 0.0;
        if self.0.len() > 4 {
            x_min = self.0[0].x;
            x_max = self.0[0].x;
            y_min = self.0[0].y;
            y_max = self.0[0].y;

            let count = self.0.len() - 4;
            for i in 1..count {
                let x = self.0[i].x;
                let y = self.0[i].y;
                x_min = x_min.min(x);
                x_max = x_max.max(x);
                y_min = y_min.min(y);
                y_max = y_max.max(y);
            }
        }

        // These are destined for storage in a 16 bit field to clamp the values to
        // fit into a 16 bit signed integer.
        Bounds {
            x_min: x_min.round().clamp(-32768.0, 32767.0),
            y_min: y_min.round().clamp(-32768.0, 32767.0),
            x_max: x_max.round().clamp(-32768.0, 32767.0),
            y_max: y_max.round().clamp(-32768.0, 32767.0),
        }
    }

    fn get_bounds_without_phantoms(&self) -> Bounds {
        let mut x_min = 0.0;
        let mut x_max = 0.0;
        let mut y_min = 0.0;
        let mut y_max = 0.0;
        if !self.0.is_empty() {
            x_min = self.0[0].x;
            x_max = self.0[0].x;
            y_min = self.0[0].y;
            y_max = self.0[0].y;

            for point in self.0.iter().skip(1) {
                let x = point.x;
                let y = point.y;
                x_min = x_min.min(x);
                x_max = x_max.max(x);
                y_min = y_min.min(y);
                y_max = y_max.max(y);
            }
        }

        Bounds {
            x_min: x_min.round().clamp(-32768.0, 32767.0),
            y_min: y_min.round().clamp(-32768.0, 32767.0),
            x_max: x_max.round().clamp(-32768.0, 32767.0),
            y_max: y_max.round().clamp(-32768.0, 32767.0),
        }
    }

    fn phantom_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        if self.0.len() < 4 {
            return None;
        }
        let phantom_points = &self.0[self.0.len() - PHANTOM_POINT_COUNT..];
        let left_side_x = phantom_points[0].x;
        let right_side_x = phantom_points[1].x;
        let top_side_y = phantom_points[2].y;
        let bottom_side_y = phantom_points[3].y;
        Some((left_side_x, right_side_x, top_side_y, bottom_side_y))
    }

    fn update_mtx(&self, plan: &Plan, new_gid: GlyphId) {
        let bounds = self.get_bounds();

        if let Some((left_side_x, right_side_x, top_side_y, bottom_side_y)) = self.phantom_bounds()
        {
            log::debug!("Phantom bounds for GID {}: left_side_x={}, right_side_x={}, top_side_y={}, bottom_side_y={}",
                new_gid, left_side_x, right_side_x, top_side_y, bottom_side_y);
            if bounds.is_empty() {
                plan.hmtx_map
                    .borrow_mut()
                    .insert(new_gid, (right_side_x as u16, left_side_x as i16));
                plan.vmtx_map
                    .borrow_mut()
                    .insert(new_gid, (top_side_y as u16, bottom_side_y as i16));
                return;
            }
            plan.head_maxp_info.borrow_mut().update_extrema(
                bounds.x_min as i16,
                bounds.y_min as i16,
                bounds.x_max as i16,
                bounds.y_max as i16,
            );

            let mut hori_aw: u16 = (right_side_x - left_side_x).ot_round();
            let lsb: i16 = (bounds.x_min - left_side_x).ot_round();
            plan.hmtx_map
                .borrow_mut()
                .insert(new_gid, (hori_aw as u16, lsb as i16));
            log::debug!(
                "Storing hmtx for gid {}: aw={}, lsb={}",
                new_gid,
                hori_aw as u16,
                lsb as i16
            );

            let vert_aw = (top_side_y - bottom_side_y).round().max(0.0);
            let tsb = (bounds.y_max - top_side_y).round();
            plan.vmtx_map
                .borrow_mut()
                .insert(new_gid, (vert_aw as u16, tsb as i16));
        }
    }
}

/// Port of Harfbuzz's Glyph::get_points() - Recursively gathers contour points with gvar deltas applied at each level.
fn get_points_harfbuzz_standalone(
    glyph: Option<&Glyph<'_>>,
    gid: GlyphId,
    glyph_accelerator: &GlyfAccelerator,
    head_maxp_info_opt: &mut Option<std::cell::RefMut<HeadMaxpInfo>>,
    comp_points_scratch: &mut Vec<ContourPoint>,
    coords: &[font_types::F2Dot14],
    shift_points_hori: bool,
    depth: usize,
    decycler: &mut HashSet<GlyphId>,
    composite_contours: &mut usize,
) -> Result<(Vec<ContourPoint>, Option<Vec<ContourPoint>>), SerializeErrorFlags> {
    log::debug!("Getting points for gid {} at depth {}", gid, depth);
    let mut all_points = Vec::new();
    let mut points_with_deltas = None;
    const HB_MAX_NESTING_LEVEL: usize = 100;
    const HB_MAX_GRAPH_EDGE_COUNT: usize = 10000;

    // Edge counter for cycle detection in the point graph
    static mut EDGE_COUNT: usize = 0;
    unsafe {
        if EDGE_COUNT > HB_MAX_GRAPH_EDGE_COUNT {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW);
        }
        EDGE_COUNT += 1;
    }

    if depth > HB_MAX_NESTING_LEVEL {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW);
    }

    if let Some(ref mut info) = head_maxp_info_opt {
        info.update_max_component_depth(depth);
    }

    // Select target buffer based on glyph type
    // Simple glyphs → all_points, Composite glyphs → comp_points_scratch (anchors only)
    // We use a scratch buffer since we're going to be accumulating them recursively.
    let is_simple = matches!(glyph, Some(Glyph::Simple(_)));
    let target_points: &mut Vec<ContourPoint> = if is_simple {
        &mut all_points
    } else {
        comp_points_scratch
    };

    let old_length = target_points.len();

    // ========== SECTION 1: Load contour points based on glyph type ==========
    // This follows Harfbuzz lines ~336-355
    match glyph {
        Some(Glyph::Simple(simple_glyph)) => {
            if depth == 0 {
                if let Some(ref mut info) = head_maxp_info_opt {
                    info.update_max_contours(simple_glyph.number_of_contours() as u16);
                }
            }
            if depth > 0 {
                let num_contours = simple_glyph.number_of_contours();
                if num_contours > 0 {
                    *composite_contours += num_contours as usize;
                }
            }

            // Collect contour points from simple glyph. No variations yet.
            let end_points = simple_glyph
                .end_pts_of_contours()
                .iter()
                .map(|e| e.get())
                .collect::<Vec<u16>>();

            target_points.extend(simple_glyph.points().enumerate().map(|(ix, p)| {
                ContourPoint::new(
                    p.x as f32,
                    p.y as f32,
                    p.on_curve,
                    end_points.contains(&(ix as u16)),
                )
            }));
        }
        Some(Glyph::Composite(composite_glyph)) => {
            // Collect composite anchor points (these hold transformation info)
            // equivalent of item.get_points() in Harfbuzz's CompositeGlyph.hh
            log::debug!(
                "Adding points to the target for a composite glyph with {} components",
                composite_glyph.components().count()
            );
            for component in composite_glyph.components() {
                match component.anchor {
                    Anchor::Point { .. } => {
                        target_points.push(ContourPoint::new(0.0, 0.0, false, true));
                    }
                    Anchor::Offset { x, y } => {
                        target_points.push(ContourPoint::new(x as f32, y as f32, false, false));
                    }
                }
            }
        }
        None => {
            // Empty glyph - nothing to collect
        }
    }

    /* Init phantom points */
    // Section should be repeated from get_all_points_without_var
    // Get glyph metrics from hmtx/vmtx tables (not instantiated, just defaults)

    let x_min = match glyph {
        Some(Glyph::Simple(sg)) => sg.x_min() as f32,
        Some(Glyph::Composite(cg)) => cg.x_min() as f32,
        None => 0.0,
    };
    let y_max = match glyph {
        Some(Glyph::Simple(sg)) => sg.y_max() as f32,
        Some(Glyph::Composite(cg)) => cg.y_max() as f32,
        None => 0.0,
    };

    let lsb = glyph_accelerator.left_side_bearing(gid);
    let h_adv = glyph_accelerator.advance_width(gid);
    let h_delta = x_min - lsb;

    let tsb = 0.0; // TODO: use vmtx if available
    let v_orig = y_max + tsb;
    let v_adv = -glyph_accelerator.units_per_em(); // TODO: use vmtx if available

    // Set phantom point coordinates (PHANTOM_LEFT, PHANTOM_RIGHT, PHANTOM_TOP, PHANTOM_BOTTOM)
    let phantoms_start = target_points.len();
    target_points.push(ContourPoint::new(h_delta, 0.0, false, true));
    target_points.push(ContourPoint::new(h_adv + h_delta, 0.0, false, true));
    target_points.push(ContourPoint::new(0.0, v_orig, false, true));
    target_points.push(ContourPoint::new(0.0, v_orig - v_adv, false, true));
    let mut phantoms = target_points[phantoms_start..phantoms_start + PHANTOM_POINT_COUNT].to_vec();

    // ========== SECTION 3: Apply gvar deltas to just-added points ==========
    if !coords.is_empty() {
        // This is ugly but will do for now.
        // if !is_simple {
        //     log::debug!("Points before gvar deltas: {:?}", target_points);
        // }
        let mut cp = ContourPoints(target_points[old_length..].to_vec());
        glyph_accelerator.apply_gvar_deltas_to_points(
            gid, coords, &mut cp,
            // scratch, gvar cache, phantom_only
        );
        target_points[old_length..].copy_from_slice(cp.0.as_slice());
        phantoms = target_points[phantoms_start..phantoms_start + PHANTOM_POINT_COUNT].to_vec();
        // if !is_simple {
        //     log::debug!("Points after applying gvar deltas: {:?}", target_points);
        // }
    }

    let anchor_points = if is_simple {
        None
    } else {
        Some(comp_points_scratch.clone())
    };

    // mainly used by CompositeGlyph calculating new X/Y offset value so no need to extend it
    // with child glyphs' points
    if points_with_deltas.is_none() && depth == 0 && !is_simple {
        if let Some(ref points) = anchor_points {
            points_with_deltas = Some(points.clone());
        }
    }

    let mut shift: f32 = 0.0;

    match glyph {
        Some(Glyph::Simple(_)) => {
            // Harfbuzz lines ~414-418
            shift = phantoms[0].x;

            if let Some(ref mut info) = head_maxp_info_opt {
                if depth == 0 {
                    info.update_max_points(
                        all_points.len() as u16 - old_length as u16 - PHANTOM_POINT_COUNT as u16,
                    );
                }
            }
        }
        Some(Glyph::Composite(composite_glyph)) => {
            // Harfbuzz lines ~419-467: This is the complex recursive section

            for (comp_index, component) in composite_glyph.components().enumerate() {
                let item_gid = component.glyph.into();

                // Skip if this component creates a cycle
                if decycler.contains(&item_gid) {
                    continue;
                }

                decycler.insert(item_gid);

                let old_count = all_points.len();

                // Recursively get points for this component (with deltas applied at ITS level)
                let use_my_metrics = component
                    .flags
                    .contains(CompositeGlyphFlags::USE_MY_METRICS);

                let component_glyph = glyph_accelerator.get_glyph(item_gid);

                // RECURSIVE CALL: get_points_harfbuzz applies deltas for THIS component
                let (mut child_points, _child_points_with_deltas) = get_points_harfbuzz_standalone(
                    component_glyph.as_ref(),
                    item_gid,
                    glyph_accelerator,
                    head_maxp_info_opt,
                    comp_points_scratch,
                    coords,
                    shift_points_hori,
                    depth + 1,
                    decycler,
                    composite_contours,
                )?;

                let comp_points_len = child_points.len();
                all_points.extend(child_points.drain(..));

                // Copy USE_MY_METRICS phantoms if needed
                if use_my_metrics && comp_points_len >= PHANTOM_POINT_COUNT {
                    let comp_phantom_start = all_points.len() - PHANTOM_POINT_COUNT;
                    for i in 0..PHANTOM_POINT_COUNT {
                        phantoms[i] = all_points[comp_phantom_start + i];
                    }
                }

                // ========== Apply component transformation ==========
                // Harfbuzz lines ~467-475: Component points are transformed by matrix + translation
                if comp_points_len > 0 {
                    let transform = component.transform;
                    let scale_x = transform.xx.to_f32();
                    let scale_y = transform.yy.to_f32();
                    let skew_xy = transform.xy.to_f32();
                    let skew_yx = transform.yx.to_f32();

                    let anchor_point = anchor_points
                        .as_ref()
                        .and_then(|points| points.get(comp_index))
                        .copied()
                        .unwrap_or_else(|| ContourPoint::new(0.0, 0.0, false, false));
                    let (dx, dy) = (anchor_point.x, anchor_point.y);

                    for i in old_count..(old_count + comp_points_len) {
                        let x = all_points[i].x;
                        let y = all_points[i].y;
                        all_points[i].x = x * scale_x + y * skew_xy + dx;
                        all_points[i].y = x * skew_yx + y * scale_y + dy;
                    }
                }

                // ========== Handle anchored components ==========
                // Harfbuzz lines ~451-463: Anchor-based positioning adjustment
                // TODO: Implement anchor point matching:
                // if (item.is_anchored() && !phantom_only) {
                //   p1 = composite reference point (in all_points)
                //   p2 = component reference point (in component's points)
                //   delta = (all_points[p1] - comp_points[p2])
                //   translate all component points by delta
                // }

                // Remove phantom points from component before continuing
                // They'll be re-added to top-level phantoms after processing all components
                if all_points.len() >= PHANTOM_POINT_COUNT {
                    all_points.truncate(all_points.len() - PHANTOM_POINT_COUNT);
                }

                decycler.remove(&item_gid);
            }

            // Re-attach top-level phantom points at the end
            // These have been potentially updated by USE_MY_METRICS components
            all_points.extend(phantoms.iter().copied());

            if let Some(ref mut info) = head_maxp_info_opt {
                if depth == 0 {
                    info.update_max_composite_contours(*composite_contours as u16);
                    info.update_max_composite_points(
                        all_points.len() as u16 - PHANTOM_POINT_COUNT as u16,
                    );
                    info.update_max_component_elements(composite_glyph.components().count() as u16);
                }
            }
            shift = phantoms[0].x;

            // Clear scratch buffer
            comp_points_scratch.clear();
        }
        None => {
            // ========== SECTION 5c: Empty glyph ==========
            // Just return phantoms, no shift needed in most cases
            shift = if all_points.len() >= PHANTOM_POINT_COUNT {
                all_points[all_points.len() - PHANTOM_POINT_COUNT].x
            } else {
                0.0
            };
        }
    }

    // ========== SECTION 6: Apply horizontal shift at top level ==========
    // Harfbuzz lines ~469-475: "Shift points horizontally by the updated left side bearing"
    // This is an undocumented rasterizer behavior that Harfbuzz maintains for compatibility
    if depth == 0 && shift_points_hori && shift != 0.0 {
        for point in all_points.iter_mut() {
            point.x -= shift;
        }
    }

    log::debug!(
        "Phantom points at end of get_points: left={}, right={}",
        phantoms[0].x,
        phantoms[1].x
    );

    log::debug!(
        "Returning {} points and {} points with deltas from depth {}",
        all_points.len(),
        points_with_deltas.as_ref().map_or(0, |v| v.len()),
        depth
    );

    Ok((all_points, points_with_deltas))
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_subset_simple_glyph_trim_padding() {
        let plan = Plan::default();
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();

        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let glyph = loca.get_glyf(GlyphId::from(1_u16), &glyf).unwrap().unwrap();

        let subset_output = subset_glyph(&glyph, &plan);
        assert_eq!(subset_output.len(), 23);
        assert_eq!(
            subset_output,
            [
                0x0, 0x1, 0x0, 0xfa, 0x0, 0x32, 0x1, 0x77, 0x0, 0x64, 0x0, 0x3, 0x0, 0x0, 0x37,
                0x33, 0x15, 0x23, 0xfa, 0x7d, 0x7d, 0x64, 0x32
            ]
        );
    }

    #[test]
    fn test_subset_composite_glyph_trim_padding() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();

        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let glyph = loca.get_glyf(GlyphId::from(4_u16), &glyf).unwrap().unwrap();
        plan.glyph_map
            .insert(GlyphId::from(1_u16), GlyphId::from(2_u16));

        let subset_glyph = subset_glyph(&glyph, &plan);
        assert_eq!(subset_glyph.len(), 20);
        assert_eq!(
            subset_glyph,
            [
                0xff, 0xff, 0x2, 0x26, 0x0, 0x7d, 0x3, 0x20, 0x0, 0xc8, 0x0, 0x46, 0x0, 0x2, 0x32,
                0x32, 0x7f, 0xff, 0x60, 0x0
            ]
        );
    }
}
