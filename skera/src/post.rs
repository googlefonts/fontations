//! impl subset() for post

use std::collections::HashMap;

use crate::{serialize::Serializer, Plan, Subset, SubsetError, SubsetFlags};
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
impl Subset for Post<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        // copy header
        s.embed_bytes(self.min_table_bytes())
            .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?;

        let glyph_names = plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES);
        //version 3 does not have any glyph names
        if !glyph_names {
            s.copy_assign(
                self.shape().version_byte_range().start,
                Version16Dot16::VERSION_3_0,
            );
        }

        if glyph_names && self.version() == Version16Dot16::VERSION_2_0 {
            subset_post_v2tail(self, plan, s)?;
        }
        Ok(())
    }
}

fn subset_post_v2tail(post: &Post, plan: &Plan, s: &mut Serializer) -> Result<(), SubsetError> {
    let Some(glyph_name_indices) = post.glyph_name_index() else {
        return Err(SubsetError::SubsetTableError(Post::TAG));
    };
    //copy numGlyphs
    s.embed(plan.num_output_glyphs as u16)
        .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?;

    //init all glyphNameIndex as 0
    let glyph_index_arr_len = plan.num_output_glyphs * 2;
    let idx_start = s
        .allocate_size(glyph_index_arr_len, false)
        .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?;

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
        s.copy_assign(i, name_idx.get());
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
        return Err(SubsetError::SubsetTableError(Post::TAG));
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
                        s.embed(len)
                            .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?;
                        s.embed_bytes(ps_name.as_bytes())
                            .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?;
                        new_idx
                    }
                };
                custom_idx
            }
        };
        s.copy_assign(out_idx, name_idx);
    }
    Ok(())
}
