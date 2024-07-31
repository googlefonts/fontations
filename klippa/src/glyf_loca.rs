//! impl subset() for glyf and loca
use crate::{estimate_subset_table_size, Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::{
    read::{
        tables::{
            glyf::{
                CompositeGlyph, CompositeGlyphFlags, Glyf,
                Glyph::{self, Composite, Simple},
                SimpleGlyph, SimpleGlyphFlags,
            },
            head::Head,
            loca::Loca,
        },
        types::GlyphId,
        FontRef, TableProvider, TopLevelTable,
    },
    FontBuilder,
};

pub fn subset_glyf_loca(
    plan: &Plan,
    font: &FontRef,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let Ok(glyf) = font.glyf() else {
        return Err(SubsetTableError(Glyf::TAG));
    };

    let Ok(loca) = font.loca(None) else {
        return Err(SubsetTableError(Loca::TAG));
    };

    let Ok(head) = font.head() else {
        return Err(SubsetTableError(Head::TAG));
    };

    let num_output_glyphs = plan.num_output_glyphs;
    let mut subset_glyphs = Vec::with_capacity(num_output_glyphs);
    let mut max_offset: u32 = 0;

    //TODO: support not_def_outline and drop_hints
    for (_new_gid, old_gid) in &plan.new_to_old_gid_list {
        match loca.get_glyf(*old_gid, &glyf) {
            Ok(g) => {
                let Some(glyph) = g else {
                    subset_glyphs.push(Vec::new());
                    continue;
                };
                let subset_glyph = subset_glyph(&glyph, plan);
                let trimmed_len = subset_glyph.len();
                max_offset += padded_size(trimmed_len) as u32;
                subset_glyphs.push(subset_glyph);
            }
            _ => {
                return Err(SubsetTableError(Glyf::TAG));
            }
        }
    }

    //TODO: support force_long_loca in the plan
    let loca_format: u8 = if max_offset < 0x1FFFF { 0 } else { 1 };

    let glyf_cap = estimate_subset_table_size(font, Glyf::TAG, plan);
    let mut glyf_out = Vec::with_capacity(glyf_cap);

    let loca_cap = estimate_subset_table_size(font, Loca::TAG, plan);
    let mut loca_out: Vec<u8> = Vec::with_capacity(loca_cap);

    if loca_format == 0 {
        loca_out.extend_from_slice(&0_u16.to_be_bytes());
        let mut offset: u16 = 0;
        for g in &subset_glyphs {
            let padded_len = padded_size(g.len());
            offset += padded_len as u16;
            let glyph_offset = offset >> 1;
            loca_out.extend_from_slice(&glyph_offset.to_be_bytes());
            glyf_out.extend_from_slice(g);
            if padded_len > g.len() {
                glyf_out.extend_from_slice(&[0]);
            }
        }
    } else {
        loca_out.extend_from_slice(&0_u32.to_be_bytes());
        let mut offset: u32 = 0;
        for g in &subset_glyphs {
            offset += g.len() as u32;
            loca_out.extend_from_slice(&offset.to_be_bytes());
            glyf_out.extend_from_slice(g);
        }
    }

    // As a special case when all glyph in the font are empty, add a zero byte to the table,
    // so that OTS doesn’t reject it, and to make the table work on Windows as well.
    // See https://github.com/khaledhosny/ots/issues/52
    if glyf_out.is_empty() {
        glyf_out.extend_from_slice(&[0]);
    }

    let Ok(head_out) = subset_head(&head, loca_format) else {
        return Err(SubsetTableError(Head::TAG));
    };

    builder.add_raw(Glyf::TAG, glyf_out);
    builder.add_raw(Loca::TAG, loca_out);
    builder.add_raw(Head::TAG, head_out);
    Ok(())
}

fn padded_size(len: usize) -> usize {
    len + len % 2
}

fn subset_glyph(glyph: &Glyph, plan: &Plan) -> Vec<u8> {
    //TODO: support set_overlaps_flag and drop_hints
    match glyph {
        Composite(comp_g) => subset_composite_glyph(comp_g, plan),
        Simple(simple_g) => subset_simple_glyph(simple_g),
    }
}

// TODO: drop_hints and set_overlaps_flag
fn subset_simple_glyph(g: &SimpleGlyph) -> Vec<u8> {
    let mut out = Vec::with_capacity(g.offset_data().len());

    let Some(num_coords) = g.end_pts_of_contours().last() else {
        return out;
    };
    let num_coords = num_coords.get() + 1;
    let mut coord_bytes: usize = 0;
    let mut coords_with_flags: u16 = 0;

    let glyph_data = g.glyph_data();
    let length = glyph_data.len();
    let mut i: usize = 0;
    while i < length {
        let flag = SimpleGlyphFlags::from_bits_truncate(glyph_data[i]);
        i += 1;

        let mut repeat: u8 = 1;
        if flag.contains(SimpleGlyphFlags::REPEAT_FLAG) {
            if i >= length {
                return out;
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
        return out;
    }
    i += coord_bytes;

    let trimmed_len =
        10 + 2 * (g.number_of_contours() as usize) + 2 + g.instruction_length() as usize + i;
    let Some(trimmed_slice) = g.offset_data().as_bytes().get(0..trimmed_len) else {
        return out;
    };
    out.extend_from_slice(trimmed_slice);
    out
}

fn subset_composite_glyph(g: &CompositeGlyph, plan: &Plan) -> Vec<u8> {
    let mut out = Vec::with_capacity(g.offset_data().len());
    out.extend_from_slice(g.offset_data().as_bytes());

    let mut more = true;
    let mut we_have_instructions = false;
    let mut i: usize = 10;
    let len: usize = out.len();

    while more {
        if i + 3 >= len {
            return Vec::new();
        }
        let flags = u16::from_be_bytes([out[i], out[i + 1]]);
        let flags = CompositeGlyphFlags::from_bits_truncate(flags);

        if flags.contains(CompositeGlyphFlags::WE_HAVE_INSTRUCTIONS) {
            we_have_instructions = true;
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

    if we_have_instructions {
        if i + 1 >= len {
            return Vec::new();
        }
        let instruction_len = u16::from_be_bytes([out[i], out[i + 1]]);
        i += 2 + instruction_len as usize;
    }

    out.truncate(i);
    out
}

fn subset_head(head: &Head, loca_format: u8) -> Result<Vec<u8>, SubsetError> {
    let mut out = Vec::new();
    out.extend_from_slice(head.offset_data().as_bytes());

    let Some(index_loca_format) = out.get_mut(50..52) else {
        return Err(SubsetTableError(Head::TAG));
    };
    index_loca_format[0] = 0;
    index_loca_format[1] = loca_format;
    Ok(out)
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
