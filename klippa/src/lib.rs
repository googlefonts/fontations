//! try to define Subset trait so I can add methods for Hmtx
//! TODO: make it generic for all tables
mod glyf_loca;
mod head;
mod hmtx;
mod maxp;
mod parsing_util;
use glyf_loca::subset_glyf_loca;
use head::subset_head;
use hmtx::subset_hmtx_hhea;
use maxp::subset_maxp;
pub use parsing_util::{parse_unicodes, populate_gids};

use fnv::FnvHashMap;
use skrifa::MetadataProvider;
use thiserror::Error;
use write_fonts::read::{
    collections::IntSet,
    tables::{
        cff::Cff,
        cff2::Cff2,
        glyf::{Glyf, Glyph},
        gpos::Gpos,
        gsub::Gsub,
        head::Head,
        loca::Loca,
        name::Name,
    },
    FontRef, TableProvider, TopLevelTable,
};
use write_fonts::types::GlyphId;
use write_fonts::types::Tag;
use write_fonts::{tables::hhea::Hhea, tables::hmtx::Hmtx, tables::maxp::Maxp, FontBuilder};

const MAX_COMPOSITE_OPERATIONS_PER_GLYPH: u8 = 64;
const MAX_NESTING_LEVEL: u8 = 64;
// Support 24-bit gids. This should probably be extended to u32::MAX but
// this causes tests to fail with 'subtract with overflow error'.
// See <https://github.com/googlefonts/fontations/issues/997>
const MAX_GID: GlyphId = GlyphId::new(0xFFFFFF);

#[derive(Clone, Copy, Debug)]
pub struct SubsetFlags(u16);

impl SubsetFlags {
    //all flags at their default value of false.
    pub const SUBSET_FLAGS_DEFAULT: Self = Self(0x0000);

    //If set hinting instructions will be dropped in the produced subset.
    //Otherwise hinting instructions will be retained.
    pub const SUBSET_FLAGS_NO_HINTING: Self = Self(0x0001);

    //If set glyph indices will not be modified in the produced subset.
    //If glyphs are dropped their indices will be retained as an empty glyph.
    pub const SUBSET_FLAGS_RETAIN_GIDS: Self = Self(0x0002);

    //If set and subsetting a CFF font the subsetter will attempt to remove subroutines from the CFF glyphs.
    pub const SUBSET_FLAGS_DESUBROUTINIZE: Self = Self(0x0004);

    //If set non-unicode name records will be retained in the subset.
    pub const SUBSET_FLAGS_NAME_LEGACY: Self = Self(0x0008);

    //If set the subsetter will set the OVERLAP_SIMPLE flag on each simple glyph.
    pub const SUBSET_FLAGS_SET_OVERLAPS_FLAG: Self = Self(0x0010);

    //If set the subsetter will not drop unrecognized tables and instead pass them through untouched.
    pub const SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED: Self = Self(0x0020);

    //If set the notdef glyph outline will be retained in the final subset.
    pub const SUBSET_FLAGS_NOTDEF_OUTLINE: Self = Self(0x0040);

    //If set the PS glyph names will be retained in the final subset.
    pub const SUBSET_FLAGS_GLYPH_NAMES: Self = Self(0x0080);

    //If set then the unicode ranges in OS/2 will not be recalculated.
    pub const SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES: Self = Self(0x0100);

    //If set don't perform glyph closure on layout substitution rules (GSUB)
    pub const SUBSET_FLAGS_NO_LAYOUT_CLOSURE: Self = Self(0x0200);

    //If set perform IUP delta optimization on the remaining gvar table's deltas.
    pub const SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS: Self = Self(0x0400);

    /// Returns `true` if all of the flags in `other` are contained within `self`.
    #[inline]
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl Default for SubsetFlags {
    fn default() -> Self {
        Self::SUBSET_FLAGS_DEFAULT
    }
}

impl PartialEq for SubsetFlags {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::ops::BitOr for SubsetFlags {
    type Output = Self;

    /// Returns the union of the two sets of flags.
    #[inline]
    fn bitor(self, other: SubsetFlags) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<u16> for SubsetFlags {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl std::ops::BitOrAssign for SubsetFlags {
    /// Adds the set of flags.
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct Plan {
    unicodes: IntSet<u32>,
    glyphset_gsub: IntSet<GlyphId>,
    glyphset_colred: IntSet<GlyphId>,
    glyphset: IntSet<GlyphId>,
    //Old->New glyph id mapping,
    glyph_map: FnvHashMap<GlyphId, GlyphId>,

    new_to_old_gid_list: Vec<(GlyphId, GlyphId)>,

    num_output_glyphs: usize,
    font_num_glyphs: usize,
    unicode_to_new_gid_list: Vec<(u32, GlyphId)>,
    codepoint_to_glyph: FnvHashMap<u32, GlyphId>,

    subset_flags: SubsetFlags,
}

impl Plan {
    pub fn new(
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
        flags: SubsetFlags,
    ) -> Self {
        let mut this = Plan {
            font_num_glyphs: get_font_num_glyphs(font),
            subset_flags: flags,
            ..Default::default()
        };

        this.populate_unicodes_to_retain(input_gids, input_unicodes, font);
        this.populate_gids_to_retain(font);
        this.create_old_gid_to_new_gid_map();

        this
    }

    fn populate_unicodes_to_retain(
        &mut self,
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
    ) {
        let charmap = font.charmap();
        if input_gids.is_empty() && input_unicodes.len() < (self.font_num_glyphs as u64) {
            let cap: usize = input_unicodes.len().try_into().unwrap_or(usize::MAX);
            self.unicode_to_new_gid_list.reserve(cap);
            self.codepoint_to_glyph.reserve(cap);
            //TODO: add support for subset accelerator?

            for cp in input_unicodes.iter() {
                match charmap.map(cp) {
                    Some(gid) => {
                        self.codepoint_to_glyph.insert(cp, gid);
                        self.unicode_to_new_gid_list.push((cp, gid));
                    }
                    None => {
                        continue;
                    }
                }
            }
        } else {
            //TODO: add support for subset accelerator?
            let cmap_unicodes = charmap.mappings().map(|t| t.0).collect::<IntSet<u32>>();
            let unicode_gid_map = charmap.mappings().collect::<FnvHashMap<u32, GlyphId>>();

            let vec_cap: u64 = input_gids.len() + input_unicodes.len();
            let vec_cap: usize = vec_cap
                .min(cmap_unicodes.len())
                .try_into()
                .unwrap_or(usize::MAX);
            self.codepoint_to_glyph.reserve(vec_cap);
            self.unicode_to_new_gid_list.reserve(vec_cap);
            for range in cmap_unicodes.iter_ranges() {
                for cp in range {
                    match unicode_gid_map.get(&cp) {
                        Some(gid) => {
                            if !input_gids.contains(*gid) && !input_unicodes.contains(cp) {
                                continue;
                            }
                            self.codepoint_to_glyph.insert(cp, *gid);
                            self.unicode_to_new_gid_list.push((cp, *gid));
                        }
                        None => {
                            continue;
                        }
                    }
                }
            }

            /* Add gids which where requested, but not mapped in cmap */
            for range in input_gids.iter_ranges() {
                if range.start().to_u32() as usize >= self.font_num_glyphs {
                    break;
                }
                let mut last = range.end().to_u32() as usize;
                if last >= self.font_num_glyphs {
                    last = self.font_num_glyphs - 1;
                }
                self.glyphset_gsub
                    .insert_range(*range.start()..=GlyphId::from(last as u32));
            }
        }
        self.glyphset_gsub
            .extend(self.unicode_to_new_gid_list.iter().map(|t| t.1));
        self.unicodes
            .extend(self.unicode_to_new_gid_list.iter().map(|t| t.0));
    }

    fn populate_gids_to_retain(&mut self, font: &FontRef) {
        //not-def
        self.glyphset_gsub.insert(GlyphId::NOTDEF);

        //glyph closure for cmap
        let cmap = font.cmap().expect("Error reading cmap table");
        cmap.closure_glyphs(&self.unicodes, &mut self.glyphset_gsub);
        remove_invalid_gids(&mut self.glyphset_gsub, self.font_num_glyphs);

        //skip glyph closure for MATH table, it's not supported yet

        //glyph closure for COLR
        self.colr_closure(font);
        remove_invalid_gids(&mut self.glyphset_colred, self.font_num_glyphs);

        /* Populate a full set of glyphs to retain by adding all referenced composite glyphs. */
        let loca = font.loca(None).expect("Error reading loca table");
        let glyf = font.glyf().expect("Error reading glyf table");
        let operation_count =
            self.glyphset_gsub.len() * (MAX_COMPOSITE_OPERATIONS_PER_GLYPH as u64);
        for gid in self.glyphset_colred.iter() {
            glyf_closure_glyphs(
                &loca,
                &glyf,
                gid,
                &mut self.glyphset,
                operation_count as i32,
                0,
            );
        }
        remove_invalid_gids(&mut self.glyphset, self.font_num_glyphs);
    }

    fn create_old_gid_to_new_gid_map(&mut self) {
        let pop = self.glyphset.len();
        self.glyph_map.reserve(pop as usize);
        self.new_to_old_gid_list.reserve(pop as usize);

        //TODO: Add support for requested_glyph_map, command line option --gid-map
        if !self
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS)
        {
            self.new_to_old_gid_list.extend(
                self.glyphset
                    .iter()
                    .zip(0u16..)
                    .map(|x| (GlyphId::from(x.1), x.0)),
            );
            self.num_output_glyphs = self.new_to_old_gid_list.len();
        } else {
            self.new_to_old_gid_list
                .extend(self.glyphset.iter().map(|x| (x, x)));
            let Some(max_glyph) = self.glyphset.last() else {
                return;
            };
            self.num_output_glyphs = max_glyph.to_u32() as usize + 1;
        }
        self.glyph_map
            .extend(self.new_to_old_gid_list.iter().map(|x| (x.1, x.0)));
    }

    fn colr_closure(&mut self, font: &FontRef) {
        if let Ok(colr) = font.colr() {
            colr.v0_closure_glyphs(&self.glyphset_gsub, &mut self.glyphset_colred);
            let mut layer_indices = IntSet::empty();
            let mut palette_indices = IntSet::empty();
            let mut variation_indices = IntSet::empty();
            let mut delta_set_indices = IntSet::empty();
            colr.v1_closure(
                &mut self.glyphset_colred,
                &mut layer_indices,
                &mut palette_indices,
                &mut variation_indices,
                &mut delta_set_indices,
            );
            colr.v0_closure_palette_indices(&self.glyphset_colred, &mut palette_indices);

            //TODO: remap layer_indices and palette_indices
            //TODO: generate varstore innermaps or something similar
        } else {
            self.glyphset_colred.union(&self.glyphset_gsub);
        }
    }
}

/// glyph closure for Composite glyphs in glyf table
/// limit the number of operations through returning an operation count
fn glyf_closure_glyphs(
    loca: &Loca,
    glyf: &Glyf,
    gid: GlyphId,
    gids_to_retain: &mut IntSet<GlyphId>,
    operation_count: i32,
    depth: u8,
) -> i32 {
    if gids_to_retain.contains(gid) {
        return operation_count;
    }
    gids_to_retain.insert(gid);

    if depth > MAX_NESTING_LEVEL {
        return operation_count;
    }
    let depth = depth + 1;

    let mut operation_count = operation_count - 1;
    if operation_count < 0 {
        return operation_count;
    }

    if let Some(Glyph::Composite(glyph)) = loca.get_glyf(gid, glyf).ok().flatten() {
        for child in glyph.components() {
            operation_count = glyf_closure_glyphs(
                loca,
                glyf,
                child.glyph.into(),
                gids_to_retain,
                operation_count,
                depth,
            );
        }
    }
    operation_count
}

fn remove_invalid_gids(gids: &mut IntSet<GlyphId>, num_glyphs: usize) {
    gids.remove_range(GlyphId::new(num_glyphs as u32)..=MAX_GID);
}

fn get_font_num_glyphs(font: &FontRef) -> usize {
    let loca = font.loca(None).expect("Error reading loca table");
    let ret = loca.len();

    let maxp = font.maxp().expect("Error reading maxp table");
    ret.max(maxp.num_glyphs() as usize)
}

#[derive(Debug, Error)]
pub enum SubsetError {
    #[error("Invalid input gid {0}")]
    InvalidGid(String),

    #[error("Invalid gid range {start}-{end}")]
    InvalidGidRange { start: u32, end: u32 },

    #[error("Invalid input unicode {0}")]
    InvalidUnicode(String),

    #[error("Invalid unicode range {start}-{end}")]
    InvalidUnicodeRange { start: u32, end: u32 },

    #[error("Subsetting table '{0}' failed")]
    SubsetTableError(Tag),
}

pub trait Subset {
    /// Subset this object. Returns `true` if the object should be retained.
    fn subset(&self, font: &FontRef, plan: &Plan) -> Result<Vec<u8>, SubsetError>;
}

pub fn subset_font(font: &FontRef, plan: &Plan) -> Result<Vec<u8>, SubsetError> {
    let mut builder = FontBuilder::default();

    for record in font.table_directory.table_records() {
        let tag = record.tag();
        subset_table(tag, font, plan, &mut builder)?;
    }
    Ok(builder.build())
}

fn subset_table<'a>(
    tag: Tag,
    font: &FontRef<'a>,
    plan: &Plan,
    builder: &mut FontBuilder<'a>,
) -> Result<(), SubsetError> {
    match tag {
        Glyf::TAG => {
            subset_glyf_loca(plan, font, builder)?;
        }
        Head::TAG => {
            //handled by glyf table if exists
            if let Ok(_glyf) = font.glyf() {
                return Ok(());
            } else {
                subset_head(font, plan, builder)?;
            }
        }
        //Skip, handled by Hmtx
        Hhea::TAG => {
            return Ok(());
        }

        Hmtx::TAG => {
            subset_hmtx_hhea(font, plan, builder)?;
        }
        //Skip, handled by glyf
        Loca::TAG => {
            return Ok(());
        }

        Maxp::TAG => {
            subset_maxp(font, plan, builder)?;
        }
        _ => {
            if let Some(data) = font.data_for_tag(tag) {
                builder.add_raw(tag, data);
            } else {
                return Err(SubsetError::SubsetTableError(tag));
            }
        }
    }
    Ok(())
}

pub fn estimate_subset_table_size(font: &FontRef, table_tag: Tag, plan: &Plan) -> usize {
    let Some(table_data) = font.data_for_tag(table_tag) else {
        return 0;
    };

    let table_len = table_data.len();
    let mut bulk: usize = 8192;
    let src_glyphs = plan.font_num_glyphs;
    let dst_glyphs = plan.num_output_glyphs;

    // ported from HB: Tables that we want to allocate same space as the source table.
    // For GSUB/GPOS it's because those are expensive to subset, so giving them more room is fine.
    let same_size: bool =
        table_tag == Gsub::TAG || table_tag == Gpos::TAG || table_tag == Name::TAG;

    if plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS)
    {
        if table_tag == Cff::TAG {
            //Add some extra room for the CFF charset
            bulk += src_glyphs * 16;
        } else if table_tag == Cff2::TAG {
            // Just extra CharString offsets
            bulk += src_glyphs * 4;
        }
    }

    if src_glyphs == 0 || same_size {
        return bulk + table_len;
    }

    bulk + table_len * ((dst_glyphs as f32 / src_glyphs as f32).sqrt() as usize)
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn populate_unicodes_wo_input_gid() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        plan.font_num_glyphs = get_font_num_glyphs(&font);

        let input_gids = IntSet::empty();
        let mut input_unicodes = IntSet::empty();
        input_unicodes.insert(0x2c_u32);
        input_unicodes.insert(0x31_u32);

        plan.populate_unicodes_to_retain(&input_gids, &input_unicodes, &font);

        assert_eq!(plan.unicodes.len(), 2);
        assert!(plan.unicodes.contains(0x2c_u32));
        assert!(plan.unicodes.contains(0x31_u32));

        assert_eq!(plan.glyphset_gsub.len(), 2);
        assert!(plan.glyphset_gsub.contains(GlyphId::new(2)));
        assert!(plan.glyphset_gsub.contains(GlyphId::new(4)));

        assert_eq!(plan.unicode_to_new_gid_list.len(), 2);
        assert_eq!(plan.unicode_to_new_gid_list[0], (0x2c_u32, GlyphId::new(2)));
        assert_eq!(plan.unicode_to_new_gid_list[1], (0x31_u32, GlyphId::new(4)));

        assert_eq!(plan.codepoint_to_glyph.len(), 2);
        assert_eq!(
            plan.codepoint_to_glyph.get(&0x2c_u32),
            Some(GlyphId::new(2)).as_ref()
        );
        assert_eq!(
            plan.codepoint_to_glyph.get(&0x31_u32),
            Some(GlyphId::new(4)).as_ref()
        );
    }

    #[test]
    fn populate_unicodes_w_input_gid() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        plan.font_num_glyphs = get_font_num_glyphs(&font);

        let mut input_gids = IntSet::empty();
        let input_unicodes = IntSet::empty();
        input_gids.insert(GlyphId::new(2));
        input_gids.insert(GlyphId::new(4));

        plan.populate_unicodes_to_retain(&input_gids, &input_unicodes, &font);
        assert_eq!(plan.unicodes.len(), 2);
        assert!(plan.unicodes.contains(0x2c_u32));
        assert!(plan.unicodes.contains(0x31_u32));

        assert_eq!(plan.glyphset_gsub.len(), 2);
        assert!(plan.glyphset_gsub.contains(GlyphId::new(2)));
        assert!(plan.glyphset_gsub.contains(GlyphId::new(4)));

        assert_eq!(plan.unicode_to_new_gid_list.len(), 2);
        assert_eq!(plan.unicode_to_new_gid_list[0], (0x2c_u32, GlyphId::new(2)));
        assert_eq!(plan.unicode_to_new_gid_list[1], (0x31_u32, GlyphId::new(4)));

        assert_eq!(plan.codepoint_to_glyph.len(), 2);
        assert_eq!(
            plan.codepoint_to_glyph.get(&0x2c_u32),
            Some(GlyphId::new(2)).as_ref()
        );
        assert_eq!(
            plan.codepoint_to_glyph.get(&0x31_u32),
            Some(GlyphId::new(4)).as_ref()
        );
    }

    #[test]
    fn glyf_closure_composite_glyphs() {
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let mut gids = IntSet::empty();

        glyf_closure_glyphs(&loca, &glyf, GlyphId::new(5), &mut gids, 64, 0);
        assert_eq!(gids.len(), 2);
        assert!(gids.contains(GlyphId::new(5)));
        assert!(gids.contains(GlyphId::new(1)));
    }

    #[test]
    fn populate_gids_wo_cmap_colr_layout() {
        let mut plan = Plan::default();
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        plan.font_num_glyphs = get_font_num_glyphs(&font);
        plan.unicodes.insert(0x2c_u32);
        plan.unicodes.insert(0x34_u32);

        plan.glyphset_gsub.insert(GlyphId::new(2));
        plan.glyphset_gsub.insert(GlyphId::new(7));

        plan.populate_gids_to_retain(&font);
        assert_eq!(plan.glyphset_gsub.len(), 3);
        assert!(plan.glyphset_gsub.contains(GlyphId::new(0)));
        assert!(plan.glyphset_gsub.contains(GlyphId::new(2)));
        assert!(plan.glyphset_gsub.contains(GlyphId::new(7)));

        assert_eq!(plan.glyphset.len(), 5);
        assert!(plan.glyphset.contains(GlyphId::new(0)));
        assert!(plan.glyphset.contains(GlyphId::new(1)));
        assert!(plan.glyphset.contains(GlyphId::new(2)));
        assert!(plan.glyphset.contains(GlyphId::new(4)));
        assert!(plan.glyphset.contains(GlyphId::new(7)));
    }
}
