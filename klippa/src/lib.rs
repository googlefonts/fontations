//! try to define Subset trait so I can add methods for Hmtx
//! TODO: make it generic for all tables
mod hhea;
mod hmtx;
mod maxp;
mod parsing_util;
pub use parsing_util::{parse_unicodes, populate_gids};

use int_set::IntSet;
use std::collections::BTreeSet;
use std::path::PathBuf;
use thiserror::Error;
use write_fonts::read::{FontRef, TableProvider, TopLevelTable};
use write_fonts::types::GlyphId;
use write_fonts::types::Tag;
use write_fonts::{
    from_obj::FromTableRef, tables::hhea::Hhea, tables::hmtx::Hmtx, tables::maxp::Maxp, FontBuilder,
};
pub struct Plan {
    glyph_ids: IntSet<GlyphId>,
    num_h_metrics: u16,
    num_output_glyphs: u16,
}

impl Plan {
    pub fn new(input_gids: &IntSet<GlyphId>, input_unicodes: &IntSet<u32>, font: &FontRef) -> Self {
        let gids = populate_unicodes_to_retain(input_gids, input_unicodes, font);
        // remove invalid gids
        let maxp = font.maxp().expect("Error reading maxp table");
        let maxp = Maxp::from_table_ref(&maxp);
        let gids: IntSet<GlyphId> = gids
            .iter()
            .filter(|gid| gid.to_u16() < maxp.num_glyphs)
            .collect();
        let num_glyphs = gids.len() as u16;

        // compute new h_metrics
        let hmtx = font.hmtx().expect("Error reading hmtx table");
        let hmtx = Hmtx::from_table_ref(&hmtx);
        let new_h_metrics = compute_new_num_h_metrics(&hmtx, &gids);

        Self {
            glyph_ids: gids,
            num_h_metrics: new_h_metrics,
            num_output_glyphs: num_glyphs,
        }
    }
}

fn populate_unicodes_to_retain(
    input_gids: &IntSet<GlyphId>,
    input_unicodes: &IntSet<u32>,
    font: &FontRef,
) -> IntSet<GlyphId> {
    let mut gids = IntSet::empty();
    gids.insert(GlyphId::new(0_u16));
    gids.extend(input_gids.iter());

    let cmap = font.cmap().expect("Error reading cmap table");
    for cp in input_unicodes.iter() {
        match cmap.map_codepoint(cp) {
            Some(gid) => {
                gids.insert(gid);
            }
            None => {
                continue;
            }
        }
    }
    gids
}

fn compute_new_num_h_metrics(hmtx_table: &Hmtx, glyph_ids: &IntSet<GlyphId>) -> u16 {
    let num_long_metrics = glyph_ids.len().min(0xFFFF);
    //TODO: we still need a BTreeSet here because we currently don't have max() and Iterator::rev() for IntSet
    let gids: BTreeSet<GlyphId> = glyph_ids.iter().collect();
    let last_gid = gids.last().unwrap().to_u16() as usize;
    let last_advance = hmtx_table
        .h_metrics
        .get(last_gid)
        .or_else(|| hmtx_table.h_metrics.last())
        .unwrap()
        .advance;

    let num_skippable_glyphs = gids
        .iter()
        .rev()
        .take_while(|gid| {
            hmtx_table
                .h_metrics
                .get(gid.to_u16() as usize)
                .or_else(|| hmtx_table.h_metrics.last())
                .unwrap()
                .advance
                == last_advance
        })
        .count();
    (num_long_metrics - num_skippable_glyphs).max(1) as u16
}

#[derive(Debug, Error)]
pub enum SubsetError {
    #[error("Invalid input gid {0}")]
    InvalidGid(String),

    #[error("Invalid gid range {start}-{end}")]
    InvalidGidRange { start: u16, end: u16 },

    #[error("Invalid input unicode {0}")]
    InvalidUnicode(String),

    #[error("Invalid unicode range {start}-{end}")]
    InvalidUnicodeRange { start: u32, end: u32 },

    #[error("Subsetting table '{0}' failed")]
    SubsetTableError(Tag),
}

pub trait Subset {
    /// Subset this object. Returns `true` if the object should be retained.
    fn subset(&mut self, plan: &Plan) -> Result<bool, SubsetError>;
}

pub fn subset_font(font: FontRef, plan: &Plan, output_file: &PathBuf) {
    let hmtx = font.hmtx().expect("Error reading hmtx table");
    let mut hmtx = Hmtx::from_table_ref(&hmtx);
    hmtx.subset(plan).expect("SUbsetting failed");
    let hmtx_bytes = write_fonts::dump_table(&hmtx).unwrap();

    let hhea = font.hhea().expect("Error reading hhea table");
    let mut hhea = Hhea::from_table_ref(&hhea);
    hhea.subset(plan).expect("Subsetting failed");
    let hhea_bytes = write_fonts::dump_table(&hhea).unwrap();

    let maxp = font.maxp().expect("Error reading maxp table");
    let mut maxp = Maxp::from_table_ref(&maxp);
    maxp.subset(plan).expect("Subsetting failed");
    let maxp_bytes = write_fonts::dump_table(&maxp).unwrap();

    let mut builder = FontBuilder::default();
    builder.add_raw(Hmtx::TAG, hmtx_bytes);
    builder.add_raw(Hhea::TAG, hhea_bytes);
    builder.add_raw(Maxp::TAG, maxp_bytes);

    builder.copy_missing_tables(font);

    std::fs::write(output_file, builder.build()).unwrap();
}
