//! impl subset() for post

use std::collections::HashMap;

use crate::{Plan, Subset, SubsetError, SubsetFlags};
use write_fonts::{
    read::{
        tables::post::{Post, DEFAULT_GLYPH_NAMES},
        FontRef, TopLevelTable,
    },
    types::{GlyphId, Version16Dot16},
    FontBuilder,
};

// reference: subset() for post in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-post-table.hh#L96
impl<'a> Subset for Post<'a> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let mut out = Vec::with_capacity(self.offset_data().len());
        // copy header
        out.extend_from_slice(self.offset_data().as_bytes().get(0..32).unwrap());

        let glyph_names = plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES);
        //version 3 does not have any glyph names
        if !glyph_names {
            let major_version = 0x3_u16.to_be_bytes();
            out.get_mut(0..2).unwrap().copy_from_slice(&major_version);
            out.get_mut(2..4).unwrap().fill(0);
        }

        if glyph_names && self.version() == Version16Dot16::VERSION_2_0 {
            subset_post_v2tail(self, plan, &mut out);
        }

        builder.add_raw(Post::TAG, out);
        Ok(())
    }
}

fn subset_post_v2tail(post: &Post, plan: &Plan, out: &mut Vec<u8>) {
    let Some(glyph_name_indices) = post.glyph_name_index() else {
        return;
    };
    //copy numGlyphs
    out.extend_from_slice(&(plan.num_output_glyphs as u16).to_be_bytes());

    //init all glyphNameIndex as 0
    let idx_start = out.len();
    let new_len = out.len() + plan.num_output_glyphs * 2;
    out.resize(new_len, 0);

    let max_old_gid = plan.glyphset.last().unwrap().to_u32() as usize;
    // for standard glyphs: name indices < 258
    let glyph_index_iter = glyph_name_indices
        .iter()
        .enumerate()
        .take(max_old_gid + 1)
        .filter(|x| x.1.get() < 258);

    for (old_gid, name_idx) in glyph_index_iter {
        //skip old_gids without new_gid mapping, they are retain-gids holes
        let Some(new_gid) = plan.glyph_map.get(&GlyphId::from(old_gid as u32)) else {
            continue;
        };
        let new_gid = new_gid.to_u32() as usize;
        if new_gid >= plan.num_output_glyphs {
            continue;
        }
        let i = idx_start + new_gid * 2;
        out.get_mut(i..i + 2)
            .unwrap()
            .copy_from_slice(&name_idx.get().to_be_bytes());
    }

    let standard_glyphs = DEFAULT_GLYPH_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| (*name, i as u16))
        .collect::<HashMap<_, _>>();

    let mut visited_names = HashMap::<&str, u16>::with_capacity(max_old_gid + 1);
    let mut i: u16 = DEFAULT_GLYPH_NAMES.len() as u16;

    // for custom glyph names
    let Some(ps_names) = post.string_data() else {
        return;
    };
    let glyph_names_iter = glyph_name_indices
        .iter()
        .enumerate()
        .take(max_old_gid + 1)
        .filter(|x| x.1.get() >= 258)
        .zip(ps_names.iter());

    for ((old_gid, _), ps_name) in glyph_names_iter {
        let Some(new_gid) = plan.glyph_map.get(&GlyphId::from(old_gid as u32)) else {
            continue;
        };
        let ps_name = ps_name.unwrap().as_str();
        let out_idx = idx_start + new_gid.to_u32() as usize * 2;

        let name_idx = match standard_glyphs.get(ps_name) {
            Some(standard_name_idx) => *standard_name_idx,
            None => {
                let custom_idx = match visited_names.get(ps_name) {
                    Some(visited_idx) => *visited_idx,
                    None => {
                        let new_idx = i;
                        visited_names.insert(ps_name, new_idx);
                        i += 1;

                        let len = ps_name.len() as u8;
                        out.extend_from_slice(&len.to_be_bytes());
                        out.extend_from_slice(ps_name.as_bytes());
                        new_idx
                    }
                };
                custom_idx
            }
        };
        out.get_mut(out_idx..out_idx + 2)
            .unwrap()
            .copy_from_slice(&name_idx.to_be_bytes());
    }
}
