use super::instance::{resolve_paint, ColorStops, ColrInstance, ResolvedPaint};
use crate::prelude::{GlyphId, LocationRef};
use read_fonts::tables::colr::{Colr, Paint};

pub fn print_color_glyph(colr: &Colr, glyph_id: GlyphId, location: LocationRef) {
    let instance = ColrInstance::new(colr.clone(), location.coords());
    let paint = instance.v1_base_glyph(glyph_id).unwrap().unwrap().0;
    print_paint(&instance, &paint, 0);
}

fn print_paint(instance: &ColrInstance, paint: &Paint, depth: usize) {
    let paint = resolve_paint(instance, paint).unwrap();
    for _ in 0..depth {
        print!("    ");
    }
    match paint {
        ResolvedPaint::Glyph { glyph_id, paint } => {
            println!("Glyph {glyph_id}");
            print_paint(instance, &paint, depth + 1);
        }
        ResolvedPaint::ColrGlyph { glyph_id } => {
            println!("ColrGlyph {glyph_id}");
        }
        ResolvedPaint::ColrLayers { range } => {
            println!("ColrLayers {:?}", range.clone());
            for i in range {
                let paint = instance.v1_layer(i).unwrap().0;
                print_paint(instance, &paint, depth + 1);
            }
        }
        ResolvedPaint::Composite {
            source_paint,
            mode,
            backdrop_paint,
        } => {
            println!("Composite {:?}", mode);
            print_paint(instance, &backdrop_paint, depth + 1);
            print_paint(instance, &source_paint, depth + 1);
        }
        ResolvedPaint::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            x2,
            y2,
            color_stops,
            extend,
        } => {
            print!(
                "LinearGradient p0: ({x0},{y0}) p1: ({x1},{y1}) p2: ({x2},{y2}) extend: {:?} ",
                extend
            );
            print_color_stops(instance, color_stops);
        }
        ResolvedPaint::RadialGradient {
            x0,
            y0,
            radius0,
            x1,
            y1,
            radius1,
            color_stops,
            extend,
        } => {
            print!("RadialGradient p0: ({x0},{y0}) r0: {radius0} p1: ({x1},{y1}) r1: {radius1} extend: {:?} ", extend);
            print_color_stops(instance, color_stops);
        }
        ResolvedPaint::SweepGradient {
            center_x,
            center_y,
            start_angle,
            end_angle,
            color_stops,
            extend,
        } => {
            print!("SweepGradient center: ({center_x},{center_y}) angle: {start_angle}->{end_angle} extend: {:?} ", extend);
            print_color_stops(instance, color_stops);
        }
        ResolvedPaint::Rotate {
            angle,
            around_center,
            paint,
        } => {
            let center = around_center.unwrap_or_default();
            println!("Rotate angle: {angle} center: ({}, {})", center.x, center.y);
            print_paint(instance, &paint, depth + 1);
        }
        ResolvedPaint::Scale {
            scale_x,
            scale_y,
            around_center,
            paint,
        } => {
            let center = around_center.unwrap_or_default();
            println!(
                "Scale x: {scale_x} y: {scale_y} center: ({}, {})",
                center.x, center.y
            );
            print_paint(instance, &paint, depth + 1);
        }
        ResolvedPaint::Skew {
            x_skew_angle,
            y_skew_angle,
            around_center,
            paint,
        } => {
            let center = around_center.unwrap_or_default();
            println!(
                "Skew x: {x_skew_angle} y: {y_skew_angle} center: ({}, {})",
                center.x, center.y
            );
            print_paint(instance, &paint, depth + 1);
        }
        ResolvedPaint::Solid {
            palette_index,
            alpha,
        } => {
            println!("Solid {palette_index} {alpha}");
        }
        ResolvedPaint::Transform {
            xx,
            yx,
            xy,
            yy,
            dx,
            dy,
            paint,
        } => {
            println!("Transform {xx} {yx} {xy} {yy} {dx} {dy}");
            print_paint(instance, &paint, depth + 1);
        }
        ResolvedPaint::Translate { dx, dy, paint } => {
            println!("Translate x: {dx} y: {dy}");
            print_paint(instance, &paint, depth + 1);
        }
    }
}

fn print_color_stops(instance: &ColrInstance, stops: ColorStops) {
    print!("stops: ");
    for (i, stop) in stops.resolve(instance).enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("({} {} {})", stop.offset, stop.palette_index, stop.alpha);
    }
    println!();
}
