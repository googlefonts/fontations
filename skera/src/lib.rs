//! try to define Subset trait so I can add methods for Hmtx
//! TODO: make it generic for all tables
mod base;
mod cblc;
mod cff;
mod cmap;
mod colr;
mod cpal;
mod fvar;
mod gdef;
mod glyf_loca;
mod gpos;
mod graph;
mod gsub;
mod gsubgpos;
mod gvar;
mod hdmx;
mod head;
mod hmtx;
mod hvar;
mod inc_bimap;
mod layout;
mod math;
mod maxp;
mod metrics;
mod name;
mod offset;
mod offset_array;
mod os2;
mod parsing_util;
mod post;
mod priority_queue;
mod repack;
mod sbix;
pub mod serialize;
mod stat;
mod unicode_closure;
mod variations;
mod vmtx;
mod vorg;
mod vvar;
use crate::repack::resolve_overflows;
use gdef::CollectUsedMarkSets;
use inc_bimap::IncBiMap;
use layout::{
    collect_feature_variations_name_ids, collect_features_with_retained_subs,
    collect_variable_feature_indices, find_duplicate_features, prune_features,
    remap_feature_indices, OpsWalk, PruneLangSysContext, SubsetLayoutContext,
};
pub use parsing_util::{
    parse_glyph_map, parse_name_ids, parse_name_languages, parse_tag_list, parse_unicodes,
    populate_gids,
};
use unicode_closure::unicode_closure;

use read_fonts::{
    collections::{int_set::Domain, IntSet},
    tables::{
        avar::Avar,
        base::Base,
        cbdt::Cbdt,
        cblc::Cblc,
        cff::Cff,
        cff2::Cff2,
        cmap::{Cmap, Dmap, PlatformId},
        colr::Colr,
        cpal::Cpal,
        cvar::Cvar,
        fvar::Fvar,
        gasp,
        gdef::Gdef,
        glyf::{Glyf, Glyph},
        gpos::Gpos,
        gsub::Gsub,
        gvar::{Gvar, GvarExtended},
        hdmx::Hdmx,
        head::Head,
        hhea::{Hhea, HheaExtended},
        hmtx::{Hmtx, HmtxExtended},
        hvar::Hvar,
        loca::Loca,
        math::Math,
        maxp::{Maxp, MaxpExtended},
        name::Name,
        os2::Os2,
        post::Post,
        sbix::Sbix,
        stat::Stat,
        variations::{ItemVariationStore, NO_VARIATION_INDEX},
        vhea::{Vhea, VheaExtended},
        vmtx::{Vmtx, VmtxExtended},
        vorg::Vorg,
        vvar::Vvar,
    },
    types::{GlyphId, NameId, Tag},
    FontRef, ReadError, TableProvider, TopLevelTable,
};
use serialize::{SerializeErrorFlags, Serializer};
use skrifa::MetadataProvider;
use thiserror::Error;
use write_fonts::FontBuilder;

/// The hash map type used in skera's API, e.g. for [`Plan::new_with_glyph_map`].
pub type FastHashMap<K, V> = std::collections::HashMap<K, V, foldhash::fast::FixedState>;
pub(crate) type FastHashSet<T> = std::collections::HashSet<T, foldhash::fast::FixedState>;

const MAX_COMPOSITE_OPERATIONS_PER_GLYPH: u8 = 64;
const MAX_NESTING_LEVEL: usize = read_fonts::limits::MAX_RECURSION_DEPTH;
// ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-limits.hh#L170>
/// The source bytes of all the glyphs retained from glyf, as a multiple of
/// the length of the table; a plan that retains more fails to subset glyf.
///
/// With an ascending loca the retained glyphs add up to at most the table.
/// loca need not ascend though, and any number of glyph ids may share the
/// same bytes, which are then walked for each of them.
const MAX_GLYF_BYTES_FACTOR: i64 = 16;
// Support 24-bit gids. This should probably be extended to u32::MAX but
// this causes tests to fail with 'subtract with overflow error'.
// See <https://github.com/googlefonts/fontations/issues/997>
const MAX_GID: GlyphId = GlyphId::new(0xFFFFFFFF);
pub(crate) const INVALID_GID: GlyphId = GlyphId::new(u32::MAX);

// ref: <https://github.com/harfbuzz/harfbuzz/blob/689383d05b2609a67efa307a9860b85cf40bf8c3/src/hb-subset-input.cc#L82>
pub static DEFAULT_LAYOUT_FEATURES: &[Tag] = &[
    // default shaper
    // common
    Tag::new(b"rvrn"),
    Tag::new(b"ccmp"),
    Tag::new(b"liga"),
    Tag::new(b"locl"),
    Tag::new(b"mark"),
    Tag::new(b"mkmk"),
    Tag::new(b"rlig"),
    //fractions
    Tag::new(b"frac"),
    Tag::new(b"numr"),
    Tag::new(b"dnom"),
    // horizontal
    Tag::new(b"calt"),
    Tag::new(b"clig"),
    Tag::new(b"curs"),
    Tag::new(b"kern"),
    Tag::new(b"rclt"),
    //vertical
    Tag::new(b"valt"),
    Tag::new(b"vert"),
    Tag::new(b"vkrn"),
    Tag::new(b"vpal"),
    Tag::new(b"vrt2"),
    //ltr
    Tag::new(b"ltra"),
    Tag::new(b"ltrm"),
    //rtl
    Tag::new(b"rtla"),
    Tag::new(b"rtlm"),
    //random
    Tag::new(b"rand"),
    //justify
    Tag::new(b"jalt"),
    //east asian spacing
    Tag::new(b"chws"),
    Tag::new(b"vchw"),
    Tag::new(b"halt"),
    Tag::new(b"vhal"),
    Tag::new(b"palt"),
    //private
    Tag::new(b"Harf"),
    Tag::new(b"HARF"),
    Tag::new(b"Buzz"),
    Tag::new(b"BUZZ"),
    //complex shapers
    //arabic
    Tag::new(b"init"),
    Tag::new(b"medi"),
    Tag::new(b"fina"),
    Tag::new(b"isol"),
    Tag::new(b"med2"),
    Tag::new(b"fin2"),
    Tag::new(b"fin3"),
    Tag::new(b"cswh"),
    Tag::new(b"mset"),
    Tag::new(b"stch"),
    //hangul
    Tag::new(b"ljmo"),
    Tag::new(b"vjmo"),
    Tag::new(b"tjmo"),
    //tibetan
    Tag::new(b"abvs"),
    Tag::new(b"blws"),
    Tag::new(b"abvm"),
    Tag::new(b"blwm"),
    //indic
    Tag::new(b"nukt"),
    Tag::new(b"akhn"),
    Tag::new(b"rphf"),
    Tag::new(b"rkrf"),
    Tag::new(b"pref"),
    Tag::new(b"blwf"),
    Tag::new(b"half"),
    Tag::new(b"abvf"),
    Tag::new(b"pstf"),
    Tag::new(b"cfar"),
    Tag::new(b"vatu"),
    Tag::new(b"cjct"),
    Tag::new(b"init"),
    Tag::new(b"pres"),
    Tag::new(b"abvs"),
    Tag::new(b"blws"),
    Tag::new(b"psts"),
    Tag::new(b"haln"),
    Tag::new(b"dist"),
    Tag::new(b"abvm"),
    Tag::new(b"blwm"),
];

//ref: <https://github.com/harfbuzz/harfbuzz/blob/5cbce05a7c/src/hb-subset-input.cc#L46>
pub static DEFAULT_DROP_TABLES: &[Tag] = &[
    // Layout disabled by default
    Tag::new(b"morx"),
    Tag::new(b"mort"),
    Tag::new(b"kerx"),
    Tag::new(b"kern"),
    // Copied from fontTools:
    Tag::new(b"JSTF"),
    Tag::new(b"DSIG"),
    Tag::new(b"EBDT"),
    Tag::new(b"EBLC"),
    Tag::new(b"EBSC"),
    Tag::new(b"SVG "),
    Tag::new(b"PCLT"),
    Tag::new(b"LTSH"),
    // Graphite tables
    Tag::new(b"Feat"),
    Tag::new(b"Glat"),
    Tag::new(b"Gloc"),
    Tag::new(b"Silf"),
    Tag::new(b"Sill"),
];

#[derive(Clone, Copy, Debug)]
pub struct SubsetFlags(u16);

impl SubsetFlags {
    /// all flags at their default value of false.
    pub const SUBSET_FLAGS_DEFAULT: Self = Self(0x0000);

    /// If set hinting instructions will be dropped in the produced subset.
    /// Otherwise hinting instructions will be retained.
    pub const SUBSET_FLAGS_NO_HINTING: Self = Self(0x0001);

    /// If set glyph indices will not be modified in the produced subset.
    /// If glyphs are dropped their indices will be retained as an empty glyph.
    pub const SUBSET_FLAGS_RETAIN_GIDS: Self = Self(0x0002);

    /// If set and subsetting a CFF font the subsetter will attempt to remove subroutines from the CFF glyphs.
    pub const SUBSET_FLAGS_DESUBROUTINIZE: Self = Self(0x0004);

    /// If set non-unicode name records will be retained in the subset.
    /// This flag is UNIMPLEMENTED yet
    pub const SUBSET_FLAGS_NAME_LEGACY: Self = Self(0x0008);

    /// If set the subsetter will set the OVERLAP_SIMPLE flag on each simple glyph.
    pub const SUBSET_FLAGS_SET_OVERLAPS_FLAG: Self = Self(0x0010);

    /// If set the subsetter will not drop unrecognized tables and instead pass them through untouched.
    pub const SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED: Self = Self(0x0020);

    /// If set the notdef glyph outline will be retained in the final subset.
    pub const SUBSET_FLAGS_NOTDEF_OUTLINE: Self = Self(0x0040);

    /// If set the PS glyph names will be retained in the final subset.
    pub const SUBSET_FLAGS_GLYPH_NAMES: Self = Self(0x0080);

    /// If set then the unicode ranges in OS/2 will not be recalculated.
    pub const SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES: Self = Self(0x0100);

    /// If set don't perform glyph closure on layout substitution rules (GSUB)
    pub const SUBSET_FLAGS_NO_LAYOUT_CLOSURE: Self = Self(0x0200);

    /// If set perform IUP delta optimization on the remaining gvar table's deltas.
    /// This flag is UNIMPLEMENTED yet
    pub const SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS: Self = Self(0x0400);

    /// If set do not pull mirrored versions of input codepoints into the subset.
    pub const SUBSET_FLAGS_NO_BIDI_CLOSURE: Self = Self(0x0800);

    /// If set enforce requirements on the output subset to allow it to be used with incremental
    /// font transfer IFTB patches. Primarily, this forces all outline data to use long (32 bit)
    /// offsets.
    pub const SUBSET_FLAGS_IFTB_REQUIREMENTS: Self = Self(0x1000);

    /// If this flag is set along side SUBSET_FLAGS_RETAIN_GIDS then the number of glyphs in the
    /// font won't be reduced as a result of subsetting. If necessary empty glyphs will be included
    /// at the end of the font to keep the number of glyphs unchanged.
    pub const SUBSET_FLAGS_RETAIN_NUM_GLYPHS: Self = Self(0x2000);

    /// The combined flag preset used by [`Plan::keep_everything`].
    pub const KEEP_EVERYTHING: Self = Self(
        Self::SUBSET_FLAGS_NOTDEF_OUTLINE.0
            | Self::SUBSET_FLAGS_GLYPH_NAMES.0
            | Self::SUBSET_FLAGS_NAME_LEGACY.0
            | Self::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES.0
            | Self::SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED.0,
    );

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
    glyphs_requested: IntSet<GlyphId>,
    glyphset_mathed: IntSet<GlyphId>,
    glyphset_gsub: IntSet<GlyphId>,
    glyphset_colred: IntSet<GlyphId>,
    glyphset: IntSet<GlyphId>,
    /// Old->New glyph id mapping,
    glyph_map: FastHashMap<GlyphId, GlyphId>,
    // Old->New glyph id (in glyph_set_gsub) mapping
    glyph_map_gsub: Vec<GlyphId>,
    /// New->Old glyph id mapping,
    reverse_glyph_map: FastHashMap<GlyphId, GlyphId>,

    new_to_old_gid_list: Vec<(GlyphId, GlyphId)>,

    num_output_glyphs: usize,
    font_num_glyphs: usize,
    unicode_to_new_gid_list: Vec<(u32, GlyphId)>,

    subset_flags: SubsetFlags,
    no_subset_tables: IntSet<Tag>,
    drop_tables: IntSet<Tag>,
    name_ids: IntSet<NameId>,
    name_languages: IntSet<u16>,
    name_table_overrides: FastHashMap<name::RecordKey, Vec<u8>>,
    layout_scripts: IntSet<Tag>,
    layout_features: IntSet<Tag>,
    /// The tables with a walk that ran out of its [`layout::OpsBudget`], which
    /// fail to subset: layout tables, and cmap and DMAP for their UVS tables.
    /// Also GDEF, COLR and BASE when their variation store has a subtable that
    /// can't be read, and glyf when the retained glyphs exceed
    /// [`MAX_GLYF_BYTES_FACTOR`].
    layout_ops_exceeded: IntSet<Tag>,

    //active old->new feature index map after removing redundant langsys and prune_features
    gsub_features: FastHashMap<u16, u16>,
    gpos_features: FastHashMap<u16, u16>,

    //active features(with duplicates) old->new feature index map, used by Script/FeatureVariations
    gsub_features_w_duplicates: FastHashMap<u16, u16>,
    gpos_features_w_duplicates: FastHashMap<u16, u16>,

    // active old->new lookup index map
    gsub_lookups: FastHashMap<u16, u16>,
    gpos_lookups: FastHashMap<u16, u16>,

    // active script-langsys
    gsub_script_langsys: FastHashMap<u16, IntSet<u16>>,
    gpos_script_langsys: FastHashMap<u16, IntSet<u16>>,

    // used_mark_sets mapping: old->new
    used_mark_sets_map: FastHashMap<u16, u16>,

    //old->new colrv1 layer index map
    colrv1_layers: FastHashMap<u32, u32>,
    //old->new CPAL palette index map
    colr_palettes: FastHashMap<u16, u16>,
    // COLR varstore retained varidx mapping
    colr_varstore_inner_maps: Vec<IncBiMap>,
    // COLR table old variation index -> (New varidx, new delta) mapping
    colr_varidx_delta_map: FastHashMap<u32, (u32, i32)>,
    // COLR table new delta set index -> new var index mapping
    colr_new_deltaset_idx_varidx_map: FastHashMap<u32, u32>,

    os2_info: Os2Info,

    //BASE table old variation index -> (New varidx, new delta) mapping
    base_varidx_delta_map: FastHashMap<u32, (u32, i32)>,
    //BASE table varstore retained varidx mapping
    base_varstore_inner_maps: Vec<IncBiMap>,

    //Old layout item variation index -> (New varidx, delta) mapping
    layout_varidx_delta_map: FastHashMap<u32, (u32, i32)>,
    //GDEF table varstore retained varidx mapping
    gdef_varstore_inner_maps: Vec<IncBiMap>,
}

#[derive(Default)]
struct Os2Info {
    min_cmap_codepoint: u32,
    max_cmap_codepoint: u32,
}

impl Plan {
    /// Select all glyphs, Unicode mappings, names, scripts, and features.
    ///
    /// Preserve glyph names, legacy naming records, the `.notdef` outline,
    /// Unicode range bits, and unrecognized tables, with no dropped tables.
    /// Tables are still subset and may be optimized or re-encoded.
    pub fn keep_everything(font: &FontRef) -> Self {
        Self::new(
            &IntSet::all(),
            &IntSet::all(),
            font,
            SubsetFlags::KEEP_EVERYTHING,
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::all(),
        )
    }

    /// Returns the original-to-subset glyph mapping, including glyphs added by closure.
    ///
    /// Iteration order is unspecified. With `SUBSET_FLAGS_RETAIN_GIDS`, every
    /// pair has identical original and subset glyph IDs.
    pub fn old_to_new_glyph_mapping(&self) -> impl Iterator<Item = (GlyphId, GlyphId)> + '_ {
        self.glyph_map.iter().map(|(&old, &new)| (old, new))
    }

    /// Returns retained Unicode codepoints and their original glyph IDs.
    pub fn unicode_to_old_glyph_mapping(&self) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        self.unicode_to_new_gid_list
            .iter()
            .filter_map(|&(unicode, new)| {
                self.reverse_glyph_map.get(&new).map(|&old| (unicode, old))
            })
    }

    /// Replace, insert, or remove a naming record, matching HarfBuzz's override API.
    ///
    /// Nonempty overrides are inserted even when their name ID, language, or
    /// platform would otherwise be filtered out. `None` or an empty string
    /// removes the record. Macintosh strings must be ASCII; other platforms
    /// use UTF-16BE. Invalid requests leave the previous override intact.
    /// Drop-table and table passthrough rules still take precedence.
    pub fn override_name_table(
        &mut self,
        name_id: NameId,
        platform_id: u16,
        encoding_id: u16,
        language_id: u16,
        text: Option<&str>,
    ) -> Result<(), SubsetError> {
        let text = text.unwrap_or_default();
        let is_macintosh = platform_id == PlatformId::Macintosh as u16;
        let capacity = if is_macintosh {
            if !text.is_ascii() {
                return Err(SubsetError::InvalidNameOverride(
                    "Macintosh overrides require ASCII text",
                ));
            }
            text.len()
        } else if text.len() <= u16::MAX as usize / 2 {
            // Twice the UTF-8 length bounds UTF-16BE, so short names always fit.
            text.len() * 2
        } else {
            // Stop counting once the encoded name is too long to store.
            text.encode_utf16().take(u16::MAX as usize / 2 + 1).count() * 2
        };
        if capacity > u16::MAX as usize {
            return Err(SubsetError::InvalidNameOverride(
                "encoded name exceeds 65535 bytes",
            ));
        }
        let bytes = if is_macintosh {
            text.as_bytes().to_vec()
        } else {
            let mut bytes = Vec::with_capacity(capacity);
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            // Avoid retaining excess capacity from the short-name upper bound.
            bytes.shrink_to_fit();
            bytes
        };
        self.name_table_overrides
            .insert((platform_id, encoding_id, language_id, name_id), bytes);
        Ok(())
    }

    /// Replaces the table tags that are copied without subsetting.
    ///
    /// Drop-table rules and hint removal still take precedence. Copying tables
    /// that contain glyph IDs is the caller's responsibility when IDs change.
    /// This only changes table serialization; it does not recompute closure.
    pub fn set_no_subset_tables(&mut self, tables: &IntSet<Tag>) {
        self.no_subset_tables.clone_from(tables);
    }

    fn has_identity_glyph_map(&self) -> bool {
        self.num_output_glyphs == self.font_num_glyphs
            && self.new_to_old_gid_list.len() == self.font_num_glyphs
            && self
                .new_to_old_gid_list
                .iter()
                .enumerate()
                .all(|(gid, &(new_gid, old_gid))| {
                    new_gid.to_u32() as usize == gid && old_gid == new_gid
                })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
        flags: SubsetFlags,
        drop_tables: &IntSet<Tag>,
        layout_scripts: &IntSet<Tag>,
        layout_features: &IntSet<Tag>,
        name_ids: &IntSet<NameId>,
        name_languages: &IntSet<u16>,
    ) -> Self {
        Self::new_impl(
            input_gids,
            input_unicodes,
            font,
            flags,
            drop_tables,
            layout_scripts,
            layout_features,
            name_ids,
            name_languages,
            &FastHashMap::default(),
        )
    }

    /// Like [`Plan::new`], but retained glyphs in `glyph_map` get the requested new glyph ids.
    ///
    /// Retained glyphs that aren't in `glyph_map` are numbered after the highest requested new
    /// glyph id, and glyph 0 always stays glyph 0. Old glyph ids in `glyph_map` aren't added to
    /// the retained glyphs; include them in `input_gids` for that.
    ///
    /// Fails if two glyphs are mapped to the same new glyph id, or if `flags` contains
    /// [`SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS`] while `glyph_map` isn't empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_glyph_map(
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
        flags: SubsetFlags,
        drop_tables: &IntSet<Tag>,
        layout_scripts: &IntSet<Tag>,
        layout_features: &IntSet<Tag>,
        name_ids: &IntSet<NameId>,
        name_languages: &IntSet<u16>,
        glyph_map: &FastHashMap<GlyphId, GlyphId>,
    ) -> Result<Self, SubsetError> {
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/a69b43c1f/src/hb-subset-plan.cc#L566>
        if !glyph_map.is_empty() {
            let new_gids: IntSet<GlyphId> = glyph_map.values().copied().collect();
            if new_gids.len() != glyph_map.len() as u64 {
                return Err(SubsetError::NonUniqueGlyphMap);
            }
            if flags.contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS) {
                return Err(SubsetError::GlyphMapWithRetainGids);
            }
        }
        Ok(Self::new_impl(
            input_gids,
            input_unicodes,
            font,
            flags,
            drop_tables,
            layout_scripts,
            layout_features,
            name_ids,
            name_languages,
            glyph_map,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn new_impl(
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
        flags: SubsetFlags,
        drop_tables: &IntSet<Tag>,
        layout_scripts: &IntSet<Tag>,
        layout_features: &IntSet<Tag>,
        name_ids: &IntSet<NameId>,
        name_languages: &IntSet<u16>,
        requested_glyph_map: &FastHashMap<GlyphId, GlyphId>,
    ) -> Self {
        let mut this = Plan {
            glyphs_requested: input_gids.clone(),
            font_num_glyphs: get_font_num_glyphs(font),
            subset_flags: flags,
            drop_tables: drop_tables.clone(),
            layout_scripts: layout_scripts.clone(),
            layout_features: layout_features.clone(),
            name_ids: name_ids.clone(),
            name_languages: name_languages.clone(),
            ..Default::default()
        };

        // ref: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-subset-input.cc#L71>
        let default_no_subset_tables = [gasp::Gasp::TAG, FPGM, PREP, VDMX, DSIG];
        this.no_subset_tables
            .extend(default_no_subset_tables.iter().copied());

        this.populate_unicodes_to_retain(input_gids, input_unicodes, font);
        this.populate_gids_to_retain(font);
        this.create_old_gid_to_new_gid_map(requested_glyph_map);

        // ref: <https://github.com/harfbuzz/harfbuzz/blob/a69b43c1f/src/hb-subset-plan.cc#L750>
        if this.subset_flags.contains(
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS,
        ) {
            // We've been requested to maintain the num glyphs count from the input font.
            this.num_output_glyphs = this.font_num_glyphs;
        }

        this.create_glyph_map_gsub();
        // Update the unicode to new gid list. A code point whose glyph isn't
        // retained is dropped, as HarfBuzz does: cmap may map it to a glyph id
        // past the glyph count, which is not a glyph of the font.
        let glyph_map = &this.glyph_map;
        this.unicode_to_new_gid_list.retain_mut(|(_, gid)| {
            let Some(new_gid) = glyph_map.get(gid) else {
                return false;
            };
            *gid = *new_gid;
            true
        });
        this.collect_base_var_indices(font);
        this
    }

    fn populate_unicodes_to_retain(
        &mut self,
        input_gids: &IntSet<GlyphId>,
        input_unicodes: &IntSet<u32>,
        font: &FontRef,
    ) {
        let mut unicodes = unicode_closure(
            input_unicodes,
            !self
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NO_BIDI_CLOSURE),
        );
        let charmap = font.charmap();
        // Symbol cmaps alias missing low-byte entries to U+F000..U+F0FF.
        // Retain the source entries needed to preserve requested aliases, but
        // respect explicit exclusions in inverted sets and direct low entries.
        if charmap.is_symbol() && !unicodes.is_inverted() {
            let direct_low: IntSet<u32> = charmap
                .mappings()
                .take_while(|(cp, _)| *cp <= 0xFF)
                .map(|(cp, _)| cp)
                .collect();
            for cp in 0..=0xFF {
                if unicodes.contains(cp)
                    && !direct_low.contains(cp)
                    && charmap.map(cp + 0xF000).is_some()
                {
                    unicodes.insert(cp + 0xF000);
                }
            }
        }
        if input_gids.is_empty() && unicodes.len() < (self.font_num_glyphs as u64) {
            let cap: usize = unicodes.len().try_into().unwrap_or(usize::MAX);
            self.unicode_to_new_gid_list.reserve(cap);
            //TODO: add support for subset accelerator?

            for cp in unicodes.iter() {
                match charmap.map(cp) {
                    Some(gid) => {
                        self.unicode_to_new_gid_list.push((cp, gid));
                    }
                    None => {
                        continue;
                    }
                }
            }
        } else {
            //TODO: add support for subset accelerator?
            let vec_cap: u64 = input_gids.len() + unicodes.len();
            let vec_cap: usize = vec_cap
                .min(self.font_num_glyphs as u64)
                .try_into()
                .unwrap_or(usize::MAX);
            self.unicode_to_new_gid_list.reserve(vec_cap);
            for (cp, gid) in charmap.mappings() {
                if !input_gids.contains(gid) && !unicodes.contains(cp) {
                    continue;
                }
                self.unicode_to_new_gid_list.push((cp, gid));
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
        self.unicode_to_new_gid_list.sort();
        self.glyphset_gsub
            .extend(self.unicode_to_new_gid_list.iter().map(|t| t.1));
        self.unicodes
            .extend(self.unicode_to_new_gid_list.iter().map(|t| t.0));

        // ref: <https://github.com/harfbuzz/harfbuzz/blob/e451e91ec3608a2ebfec34d0c4f0b3d880e00e33/src/hb-subset-plan.cc#L802>
        self.os2_info.min_cmap_codepoint = self.unicodes.first().unwrap_or(0xFFFF_u32);
        self.os2_info.max_cmap_codepoint = self.unicodes.last().unwrap_or(0xFFFF_u32);

        self.collect_variation_selectors(font, &unicodes);
    }

    fn collect_variation_selectors(&mut self, font: &FontRef, input_unicodes: &IntSet<u32>) {
        for cmap in [
            font.cmap().ok(),
            font.dmap().ok().map(|dmap| dmap.as_cmap()),
        ]
        .into_iter()
        .flatten()
        {
            let Some((_, subtable)) = cmap.uvs_subtable() else {
                continue;
            };
            self.unicodes.extend(
                subtable
                    .var_selector()
                    .iter()
                    .map(|s| s.var_selector().to_u32())
                    .filter(|v| input_unicodes.contains(*v)),
            );
        }
    }

    /// Counts what the glyph closure and the subsetter read of the UVS tables of
    /// `cmap`, the `table_tag` table, and records in the plan if that is more than
    /// its budget allows: the table then fails to subset.
    fn uvs_ops_within_budget(&mut self, cmap: &Cmap, table_tag: Tag) -> bool {
        let mut budget = layout::OpsBudget::new(cmap.offset_data());
        let within_budget = crate::cmap::count_uvs_ops(cmap, &self.unicodes, &mut budget.walk());
        budget.record(table_tag, self);
        within_budget
    }

    fn populate_gids_to_retain(&mut self, font: &FontRef) {
        //not-def
        self.glyphset_gsub.insert(GlyphId::NOTDEF);

        //glyph closure for cmap
        if let Ok(cmap) = font.cmap() {
            if self.uvs_ops_within_budget(&cmap, Cmap::TAG) {
                cmap.closure_glyphs(&self.unicodes, &mut self.glyphset_gsub);
            }
        }
        if let Ok(dmap) = font.dmap() {
            // Keep cmap variants too: a zero DMAP variant falls back to cmap.
            let dmap = dmap.as_cmap();
            if self.uvs_ops_within_budget(&dmap, Dmap::TAG) {
                dmap.closure_glyphs(&self.unicodes, &mut self.glyphset_gsub);
            }
        }

        // Preserve MATH-to-GSUB dependencies before layout closure.
        if !self.drop_tables.contains(Math::TAG) {
            math::closure(font, &mut self.glyphset_gsub);
            remove_invalid_gids(&mut self.glyphset_gsub, self.font_num_glyphs);
        }
        // layout closure
        self.layout_populate_gids_to_retain(font);
        remove_invalid_gids(&mut self.glyphset_gsub, self.font_num_glyphs);

        // GSUB outputs may have MATH metrics and constructions of their own.
        // These new variants are rendering glyphs; do not feed them into GSUB.
        self.glyphset_mathed = self.glyphset_gsub.clone();
        if !self.drop_tables.contains(Math::TAG) {
            math::closure(font, &mut self.glyphset_mathed);
            remove_invalid_gids(&mut self.glyphset_mathed, self.font_num_glyphs);
        }
        self.glyphset_colred = self.glyphset_mathed.clone();

        //glyph closure for COLR
        if !self.drop_tables.contains(Tag::new(b"COLR")) {
            self.colr_closure(font);
            remove_invalid_gids(&mut self.glyphset_colred, self.font_num_glyphs);
        } else {
            self.glyphset_colred = self.glyphset_mathed.clone();
        }

        /* Populate a full set of glyphs to retain by adding all referenced composite glyphs. */
        if let Ok((glyf, loca)) = font.selected_glyf_loca(None) {
            let operation_count =
                self.glyphset_mathed.len() * (MAX_COMPOSITE_OPERATIONS_PER_GLYPH as u64);
            let mut bytes_budget =
                (glyf.offset_data().len() as i64).saturating_mul(MAX_GLYF_BYTES_FACTOR);
            for gid in self.glyphset_colred.iter() {
                glyf_closure_glyphs(
                    &loca,
                    &glyf,
                    gid,
                    &mut self.glyphset,
                    operation_count as i32,
                    &mut bytes_budget,
                    0,
                );
            }
            if bytes_budget < 0 {
                self.layout_ops_exceeded.insert(Glyf::TAG);
            }
            remove_invalid_gids(&mut self.glyphset, self.font_num_glyphs);
        } else {
            self.glyphset = self.glyphset_colred.clone();
            // seac components cannot add anything when every valid glyph is
            // already retained.
            if !self.drop_tables.contains(Cff::TAG)
                && self.glyphset.len() != self.font_num_glyphs as u64
            {
                cff::closure(font, &mut self.glyphset);
                remove_invalid_gids(&mut self.glyphset, self.font_num_glyphs);
            }
        }

        self.nameid_closure(font);
        self.collect_layout_var_indices(font);
    }

    fn layout_populate_gids_to_retain(&mut self, font: &FontRef) {
        if !self.drop_tables.contains(Tag::new(b"GSUB")) {
            if let Ok(gsub) = font.gsub() {
                gsub.closure_glyphs_lookups_features(self);
            }
        }

        if !self.drop_tables.contains(Tag::new(b"GPOS")) {
            if let Ok(gpos) = font.gpos() {
                gpos.closure_glyphs_lookups_features(self);
            }
        }
    }

    // ref: <https://github.com/harfbuzz/harfbuzz/blob/a69b43c1f/src/hb-subset-plan.cc#L552>
    fn create_old_gid_to_new_gid_map(
        &mut self,
        requested_glyph_map: &FastHashMap<GlyphId, GlyphId>,
    ) {
        let pop = self.glyphset.len();
        self.glyph_map.reserve(pop as usize);
        self.reverse_glyph_map.reserve(pop as usize);
        self.new_to_old_gid_list.reserve(pop as usize);

        if !requested_glyph_map.is_empty() {
            let mut max_glyph = 0;
            for old_gid in self.glyphset.iter() {
                if old_gid == GlyphId::NOTDEF {
                    self.new_to_old_gid_list
                        .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
                    continue;
                }
                if let Some(new_gid) = requested_glyph_map.get(&old_gid) {
                    max_glyph = max_glyph.max(new_gid.to_u32());
                    self.new_to_old_gid_list.push((*new_gid, old_gid));
                }
            }
            self.new_to_old_gid_list.sort_unstable();

            // Anything that wasn't mapped by the requested mapping should
            // be placed after the requested mapping.
            for old_gid in self.glyphset.iter() {
                if old_gid != GlyphId::NOTDEF && !requested_glyph_map.contains_key(&old_gid) {
                    max_glyph += 1;
                    self.new_to_old_gid_list
                        .push((GlyphId::new(max_glyph), old_gid));
                }
            }
            self.num_output_glyphs = max_glyph as usize + 1;
        } else if !self
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS)
        {
            self.new_to_old_gid_list.extend(
                self.glyphset
                    .iter()
                    .zip(0u32..)
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
        self.reverse_glyph_map
            .extend(self.new_to_old_gid_list.iter().map(|x| (x.0, x.1)));
    }

    fn create_glyph_map_gsub(&mut self) {
        let Some(max_gid) = self.glyphset_gsub.last() else {
            return;
        };
        self.glyph_map_gsub
            .resize(max_gid.to_u32() as usize + 1, INVALID_GID);
        for g in self.glyphset_gsub.iter() {
            if let Some(new_gid) = self.glyph_map.get(&g) {
                self.glyph_map_gsub[g.to_u32() as usize] = *new_gid;
            }
        }
    }

    fn colr_closure(&mut self, font: &FontRef) {
        if let Ok(colr) = font.colr() {
            colr.v0_closure_glyphs(&self.glyphset_mathed, &mut self.glyphset_colred);
            let mut layer_indices = IntSet::empty();
            let mut palette_indices = IntSet::empty();
            let mut variation_indices = IntSet::empty();
            colr.v1_closure(
                &mut self.glyphset_colred,
                &mut layer_indices,
                &mut palette_indices,
                &mut variation_indices,
            );

            colr.v0_closure_palette_indices(&self.glyphset_colred, &mut palette_indices);
            let _ = std::mem::replace(&mut self.colrv1_layers, remap_indices(layer_indices));
            let _ = std::mem::replace(
                &mut self.colr_palettes,
                remap_palette_indices(palette_indices),
            );

            if variation_indices.is_empty() {
                return;
            }

            // generate 3 maps:
            // colr_varidx_delta_map
            // When delta set index map is not included, it's a mapping from varIdx-> (new varIdx,delta).
            // Otherwise, it's a mapping from old delta set idx-> (new delta set idx, delta).
            // Mapping delta set indices is the same as gid mapping.
            //
            // colr_varstore_inner_maps:
            // mapping from old varidx -> new varidx
            //
            // colr_new_deltaset_idx_varidx_map:
            // generate new delta set idx-> new var_idx map if DeltsSetIndexMap exists
            if let Some(Ok(var_store)) = colr.item_variation_store() {
                let Ok(delta_set_counts) = delta_set_counts(&var_store) else {
                    self.layout_ops_exceeded.insert(Colr::TAG);
                    return;
                };
                let Ok(var_index_map) = colr.var_index_map().transpose() else {
                    return;
                };

                let mut delta_set_indices = IntSet::empty();
                let mut deltaset_idx_var_idx_map = FastHashMap::default();
                // when a DeltaSetIndexMap is included, collected variation indices are actually delta set indices,
                // we need to map them into variation indices
                if let Some(var_index_map) = &var_index_map {
                    delta_set_indices.extend(variation_indices.iter());
                    variation_indices.clear();
                    // at most one entry per delta set index
                    deltaset_idx_var_idx_map.reserve(delta_set_indices.len() as usize);
                    for idx in delta_set_indices.iter() {
                        if let Ok(var_idx) = var_index_map.get(idx) {
                            let var_idx = ((var_idx.outer as u32) << 16) + var_idx.inner as u32;
                            variation_indices.insert(var_idx);
                            deltaset_idx_var_idx_map.insert(idx, var_idx);
                        }
                    }
                }
                remap_variation_indices(
                    &delta_set_counts,
                    &variation_indices,
                    &mut self.colr_varidx_delta_map,
                );
                generate_varstore_inner_maps(
                    &variation_indices,
                    &delta_set_counts,
                    &mut self.colr_varstore_inner_maps,
                );

                // if DeltaSetIndexMap exists, we need to use deltaset index instead of var_idx
                if var_index_map.is_some() {
                    let (new_deltaset_idx_varidx_map, deltaset_idx_delta_map) =
                        remap_delta_set_indices(
                            &delta_set_indices,
                            &deltaset_idx_var_idx_map,
                            &self.colr_varidx_delta_map,
                        );
                    let _ = std::mem::replace(
                        &mut self.colr_new_deltaset_idx_varidx_map,
                        new_deltaset_idx_varidx_map,
                    );
                    let _ =
                        std::mem::replace(&mut self.colr_varidx_delta_map, deltaset_idx_delta_map);
                }
            }
        } else {
            self.glyphset_colred.union(&self.glyphset_mathed);
        }
    }

    fn nameid_closure(&mut self, font: &FontRef) {
        if !self.drop_tables.contains(Tag::new(b"STAT")) {
            if let Ok(stat) = font.stat() {
                stat.collect_name_ids(self);
            }
        };

        //TODO: skip fvar table when all axes are pinned
        if !self.drop_tables.contains(Tag::new(b"fvar")) {
            if let Ok(fvar) = font.fvar() {
                fvar.collect_name_ids(self);
            }
        }

        if !self.drop_tables.contains(Tag::new(b"CPAL")) {
            if let Ok(cpal) = font.cpal() {
                cpal.collect_name_ids(self);
            }
        }

        if !self.drop_tables.contains(Tag::new(b"GSUB")) {
            if let Ok(gsub) = font.gsub() {
                gsub.collect_name_ids(self);
            }
        }

        if !self.drop_tables.contains(Tag::new(b"GPOS")) {
            if let Ok(gpos) = font.gpos() {
                gpos.collect_name_ids(self);
            }
        }
    }

    fn collect_layout_var_indices(&mut self, font: &FontRef) {
        if self.drop_tables.contains(Tag::new(b"GDEF")) {
            return;
        }
        let Ok(gdef) = font.gdef() else {
            return;
        };
        // HarfBuzz doesn't subset a table that its sanitizer rejects for what its
        // offsets share.
        let mut budget = layout::OpsBudget::new(gdef.offset_data());
        if !layout::count_gdef_ops(&gdef, &mut budget) {
            budget.record(Gdef::TAG, self);
            return;
        }

        let mut used_mark_sets = IntSet::empty();
        gdef.collect_used_mark_sets(self, &mut used_mark_sets);
        let _ = std::mem::replace(&mut self.used_mark_sets_map, remap_indices(used_mark_sets));

        let Some(Ok(var_store)) = gdef.item_var_store() else {
            return;
        };
        let mut varidx_set = IntSet::empty();
        gdef.collect_variation_indices(&mut CollectVariationIndicesContext::new(
            self,
            &mut varidx_set,
        ));

        if let Ok(gpos) = font.gpos() {
            let mut budget = layout::OpsBudget::new(gpos.offset_data());
            gpos.collect_variation_indices(
                &mut CollectVariationIndicesContext::new(self, &mut varidx_set)
                    .with_walk(budget.walk()),
            );
            budget.record(Gpos::TAG, self);
            if let Some(Ok(variations)) = gpos.feature_variations() {
                let mut budget = layout::OpsBudget::new(gpos.offset_data());
                layout::collect_feature_variation_condition_indices(
                    &variations,
                    &self.gpos_features_w_duplicates,
                    &self.gpos_lookups,
                    &mut varidx_set,
                    &mut budget,
                );
                budget.record(Gpos::TAG, self);
            }
        }
        if let Ok(gsub) = font.gsub() {
            if let Some(Ok(variations)) = gsub.feature_variations() {
                let mut budget = layout::OpsBudget::new(gsub.offset_data());
                layout::collect_feature_variation_condition_indices(
                    &variations,
                    &self.gsub_features_w_duplicates,
                    &self.gsub_lookups,
                    &mut varidx_set,
                    &mut budget,
                );
                budget.record(Gsub::TAG, self);
            }
        }
        if !self.drop_tables.contains(Math::TAG) {
            if let Ok(math) = font.math() {
                math.collect_variation_indices(&mut CollectVariationIndicesContext::new(
                    self,
                    &mut varidx_set,
                ));
            }
        }

        let Ok(delta_set_counts) = delta_set_counts(&var_store) else {
            self.layout_ops_exceeded.insert(Gdef::TAG);
            return;
        };
        remap_variation_indices(
            &delta_set_counts,
            &varidx_set,
            &mut self.layout_varidx_delta_map,
        );

        generate_varstore_inner_maps(
            &varidx_set,
            &delta_set_counts,
            &mut self.gdef_varstore_inner_maps,
        );
    }

    fn collect_base_var_indices(&mut self, font: &FontRef) {
        if self.drop_tables.contains(Tag::new(b"BASE")) {
            return;
        }

        if font.fvar().is_err() {
            return;
        }
        let Ok(base) = font.base() else {
            return;
        };

        let Some(Ok(var_store)) = base.item_var_store() else {
            return;
        };

        let mut varidx_set = IntSet::empty();
        // Counts what it reads as subsetting does, which then runs out of
        // budget wherever this walk does.
        base.collect_variation_indices(
            &mut CollectVariationIndicesContext::new(self, &mut varidx_set)
                .with_walk(OpsWalk::new(base.offset_data())),
        );

        let Ok(delta_set_counts) = delta_set_counts(&var_store) else {
            self.layout_ops_exceeded.insert(Base::TAG);
            return;
        };
        remap_variation_indices(
            &delta_set_counts,
            &varidx_set,
            &mut self.base_varidx_delta_map,
        );
        generate_varstore_inner_maps(
            &varidx_set,
            &delta_set_counts,
            &mut self.base_varstore_inner_maps,
        );
    }
}

// TODO: when instancing, calculate delta value and set new varidx to NO_VARIATIONS_IDX if all axes are pinned
/// The number of delta sets that each ItemVariationData of `var_store` has.
///
/// A variation index can name a delta set that its subtable doesn't have,
/// which reads as no delta. Such a row is not retained: see
/// [`remap_variation_indices`] and [`generate_varstore_inner_maps`], which take
/// these counts. HVAR and VVAR, whose index maps can't say "no delta", retain
/// one row for all of them.
///
/// Fails on a subtable that can't be read. HarfBuzz's sanitizer rejects the
/// table of such a store, and the table then fails to subset.
// ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-ot-layout-common.hh#L3658>
pub(crate) fn delta_set_counts(var_store: &ItemVariationStore) -> Result<Vec<u32>, ReadError> {
    var_store
        .item_variation_data()
        .iter()
        .map(|var_data| {
            let var_data = var_data.transpose()?;
            Ok(var_data.map_or(0, |var_data| u32::from(var_data.item_count())))
        })
        .collect()
}

// ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-subset-plan-var.cc#L421>
fn remap_variation_indices(
    delta_set_counts: &[u32],
    varidx_set: &IntSet<u32>,
    varidx_delta_map: &mut FastHashMap<u32, (u32, i32)>,
) {
    if delta_set_counts.is_empty() || varidx_set.is_empty() {
        return;
    }

    varidx_delta_map.reserve(varidx_set.len() as usize);
    let mut new_major: u32 = 0;
    let mut new_minor: u32 = 0;
    let mut last_major = None;
    for var_idx in varidx_set.iter() {
        let major = var_idx >> 16;
        let Some(delta_set_count) = delta_set_counts.get(major as usize) else {
            break;
        };
        // Keep in sync with generate_varstore_inner_maps: a row that the store
        // doesn't have is not retained, so such an index keeps its meaning (no
        // delta) instead of taking a new row.
        if (var_idx & 0xFFFF) >= *delta_set_count {
            varidx_delta_map.insert(var_idx, (NO_VARIATION_INDEX, 0));
            continue;
        }

        if last_major.is_some_and(|last_major| last_major != major) {
            new_minor = 0;
            new_major += 1;
        }

        let new_idx = (new_major << 16) + new_minor;
        varidx_delta_map.insert(var_idx, (new_idx, 0));

        new_minor += 1;
        last_major = Some(major);
    }
}

// ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-subset-plan-var.cc#L41>
fn generate_varstore_inner_maps(
    varidx_set: &IntSet<u32>,
    delta_set_counts: &[u32],
    inner_maps: &mut Vec<IncBiMap>,
) {
    if varidx_set.is_empty() || delta_set_counts.is_empty() {
        return;
    }

    inner_maps.resize_with(delta_set_counts.len(), Default::default);
    // The set gives the indices of one major after those of another, so gather the minors of
    // a major and add them together, which makes room for all of them at once.
    let mut minors = Vec::new();
    let mut cur_major = 0;
    for idx in varidx_set.iter() {
        let major = idx >> 16;
        let Some(delta_set_count) = delta_set_counts.get(major as usize) else {
            break;
        };

        if major != cur_major {
            inner_maps[cur_major as usize].extend(minors.drain(..));
            cur_major = major;
        }
        // An index past the delta sets of its subtable reads as no delta: don't
        // retain rows that the store doesn't have.
        let minor = idx & 0xFFFF;
        if minor < *delta_set_count {
            minors.push(minor);
        }
    }
    inner_maps[cur_major as usize].extend(minors);
}
//
fn remap_delta_set_indices(
    delta_set_indices: &IntSet<u32>,
    deltaset_idx_var_idx_map: &FastHashMap<u32, u32>,
    varidx_delta_map: &FastHashMap<u32, (u32, i32)>,
) -> (FastHashMap<u32, u32>, FastHashMap<u32, (u32, i32)>) {
    // at most one entry per delta set index in each map
    let capacity = delta_set_indices.len() as usize;
    let mut new_deltaset_idx_varidx_map =
        FastHashMap::with_capacity_and_hasher(capacity, foldhash::fast::FixedState::default());
    let mut deltaset_idx_delta_map =
        FastHashMap::with_capacity_and_hasher(capacity, foldhash::fast::FixedState::default());
    let mut new_idx = 0_u32;

    for deltaset_idx in delta_set_indices.iter() {
        let Some(&var_idx) = deltaset_idx_var_idx_map.get(&deltaset_idx) else {
            continue;
        };

        let (new_var_idx, delta) = if var_idx == NO_VARIATION_INDEX {
            (var_idx, 0)
        } else if let Some((new_idx, d)) = varidx_delta_map.get(&var_idx) {
            (*new_idx, *d)
        } else {
            continue;
        };

        new_deltaset_idx_varidx_map.insert(new_idx, new_var_idx);
        deltaset_idx_delta_map.insert(deltaset_idx, (new_idx, delta));
        new_idx += 1;
    }
    (new_deltaset_idx_varidx_map, deltaset_idx_delta_map)
}

/// glyph closure for Composite glyphs in glyf table
/// limit the number of operations through returning an operation count
///
/// `bytes_budget` is what is left of [`MAX_GLYF_BYTES_FACTOR`], and is
/// negative once the retained glyphs exceed it.
// ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-subset-plan.cc#L395>
fn glyf_closure_glyphs(
    loca: &Loca,
    glyf: &Glyf,
    gid: GlyphId,
    gids_to_retain: &mut IntSet<GlyphId>,
    operation_count: i32,
    bytes_budget: &mut i64,
    depth: usize,
) -> i32 {
    if gids_to_retain.contains(gid) {
        return operation_count;
    }
    gids_to_retain.insert(gid);

    let glyph = loca.get(gid, glyf).and_then(|g| g.into_glyph());

    // Every retained glyph is walked here and again when glyf is subset.
    // Charge its untrimmed bytes before the operation count can bail out,
    // so that a glyph retained past that point is still paid for.
    let glyph_len = glyph.as_ref().map_or(0, |g| g.offset_data().len());
    *bytes_budget -= glyph_len as i64;
    if *bytes_budget < 0 {
        return operation_count;
    }

    if depth > MAX_NESTING_LEVEL {
        return operation_count;
    }
    let depth = depth + 1;

    let mut operation_count = operation_count - 1;
    if operation_count < 0 {
        return operation_count;
    }

    if let Some(Glyph::Composite(glyph)) = glyph {
        for child in glyph.components() {
            operation_count = glyf_closure_glyphs(
                loca,
                glyf,
                child.glyph,
                gids_to_retain,
                operation_count,
                bytes_budget,
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
    let ret = font
        .selected_glyf_loca(None)
        .map(|(_, loca)| loca.len())
        .unwrap_or_default();
    // A font without a readable maxp table has no glyphs, as it has for HarfBuzz.
    let maxp_num_glyphs = font
        .selected_maxp()
        .map(|maxp| maxp.num_glyphs() as usize)
        .unwrap_or_default();
    // MAXP states up to 2^24 glyphs in 7 bytes, and this count alone bounds the
    // glyph set, and so the per-glyph maps of the plan. Every glyph of a font
    // takes at least a byte of some per-glyph table (loca, hmtx, CharStrings),
    // so a count beyond what maxp can state is limited to the size of the font.
    // HarfBuzz has no such limit.
    let limit = font.data().len().max(u16::MAX as usize);
    ret.max(maxp_num_glyphs.min(limit))
}

pub(crate) fn remap_indices<T: Domain + std::cmp::Eq + std::hash::Hash + TryFrom<usize>>(
    indices: IntSet<T>,
) -> FastHashMap<T, T> {
    // filter_map gives no size hint, so reserve up front to avoid rehashing as the
    // map grows (e.g. tens of thousands of COLRv1 layers)
    let mut map = FastHashMap::with_capacity_and_hasher(
        indices.len() as usize,
        foldhash::fast::FixedState::default(),
    );
    map.extend(
        indices
            .iter()
            .enumerate()
            // Note: new indices that don't fit in `T` are dropped rather than
            // silently truncated; downstream lookups will then miss and surface a
            // serialization error instead of emitting a wrong index.
            .filter_map(|(new_idx, old_idx)| Some((old_idx, T::try_from(new_idx).ok()?))),
    );
    map
}

fn remap_palette_indices(indices: IntSet<u16>) -> FastHashMap<u16, u16> {
    indices
        .iter()
        .enumerate()
        .map(|x| {
            if x.1 == 0xFFFF {
                (0xFFFF, 0xFFFF)
            } else {
                (x.1, x.0 as u16)
            }
        })
        .collect()
}

/// mutable struct, updated during table subsetting
/// some tables depend on other tables' subset output
#[derive(Default)]
pub struct SubsetState {
    // whether GDEF ItemVariationStore is retained after subsetting
    has_gdef_varstore: bool,
    // number of bytes that can still be copied unmodified, see passthrough_table
    passthrough_budget: usize,
}

#[derive(Debug, Error)]
pub enum SubsetError {
    #[error("Invalid name override: {0}")]
    InvalidNameOverride(&'static str),
    #[error("Invalid input gid {0}")]
    InvalidGid(String),

    #[error("Invalid gid range {start}-{end}")]
    InvalidGidRange { start: u32, end: u32 },

    #[error("Invalid input unicode {0}")]
    InvalidUnicode(String),

    #[error("Invalid unicode range {start}-{end}")]
    InvalidUnicodeRange { start: u32, end: u32 },

    #[error("Invalid tag {0}")]
    InvalidTag(String),

    #[error("Invalid ID {0}")]
    InvalidId(String),

    #[error("Invalid glyph map entry {0}, expected old_gid:new_gid")]
    InvalidGlyphMapEntry(String),

    #[error("Subsetting table '{0}' failed")]
    SubsetTableError(Tag),

    #[error("The provided custom glyph mapping is not unique")]
    NonUniqueGlyphMap,

    #[error("SUBSET_FLAGS_RETAIN_GIDS cannot be set if a custom glyph mapping has been provided")]
    GlyphMapWithRetainGids,
}

pub trait NameIdClosure {
    /// collect name_ids
    fn collect_name_ids(&self, plan: &mut Plan);
}

/// What collecting the variation indices of a table reads and fills in, as
/// HarfBuzz's `hb_collect_variation_indices_context_t`.
pub(crate) struct CollectVariationIndicesContext<'a, 'w> {
    pub(crate) plan: &'a Plan,
    pub(crate) varidx_set: &'a mut IntSet<u32>,
    /// What the walk may still read of the tables that offsets can share:
    /// anything, unless [`Self::with_walk`] said otherwise.
    pub(crate) walk: OpsWalk<'w>,
}

impl<'a, 'w> CollectVariationIndicesContext<'a, 'w> {
    pub(crate) fn new(plan: &'a Plan, varidx_set: &'a mut IntSet<u32>) -> Self {
        Self {
            plan,
            varidx_set,
            walk: OpsWalk::default(),
        }
    }

    /// Limits what the walk may read of the tables that offsets can share.
    pub(crate) fn with_walk(self, walk: OpsWalk<'w>) -> Self {
        Self { walk, ..self }
    }
}

pub(crate) trait CollectVariationIndices {
    fn collect_variation_indices(&self, c: &mut CollectVariationIndicesContext);
}

pub(crate) trait LayoutClosure {
    /// Remove unreferenced features
    fn prune_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
        budget: &mut layout::OpsBudget,
    ) -> IntSet<u16>;

    /// Return a duplicate feature(after subsetting) index map
    /// feature index -> the first index of all duplicates for this feature
    fn find_duplicate_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
    ) -> FastHashMap<u16, u16>;

    //remove unreferenced langsys and return (script->langsys mapping, retained feature indices)
    fn prune_langsys(
        &self,
        duplicate_feature_index_map: &FastHashMap<u16, u16>,
        layout_scripts: &IntSet<Tag>,
    ) -> (FastHashMap<u16, IntSet<u16>>, IntSet<u16>);

    fn closure_glyphs_lookups_features(&self, plan: &mut Plan);
}

pub const CVT: Tag = Tag::new(b"cvt ");
pub const DSIG: Tag = Tag::new(b"DSIG");
pub const EBSC: Tag = Tag::new(b"EBSC");
pub const FPGM: Tag = Tag::new(b"fpgm");
pub const GLAT: Tag = Tag::new(b"Glat");
pub const GLOC: Tag = Tag::new(b"Gloc");
pub const JSTF: Tag = Tag::new(b"JSTF");
pub const LTSH: Tag = Tag::new(b"LTSH");
pub const MORX: Tag = Tag::new(b"morx");
pub const MORT: Tag = Tag::new(b"mort");
pub const KERX: Tag = Tag::new(b"kerx");
pub const KERN: Tag = Tag::new(b"kern");
pub const PCLT: Tag = Tag::new(b"PCLT");
pub const PREP: Tag = Tag::new(b"prep");
pub const SILF: Tag = Tag::new(b"Silf");
pub const SILL: Tag = Tag::new(b"Sill");
pub const VDMX: Tag = Tag::new(b"VDMX");
pub const MVAR: Tag = Tag::new(b"MVAR");
// This trait is implemented for all font top-level tables
pub trait Subset {
    /// Subset this table, if successful a subset version of this table will be added to builder
    fn subset(
        &self,
        _plan: &Plan,
        _font: &FontRef,
        _s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        Ok(())
    }

    /// Subset this table with a mutable Subsetstate
    /// This is needed when some tables have dependencies on other table's subset output
    fn subset_with_state(
        &self,
        _plan: &Plan,
        _font: &FontRef,
        _state: &mut SubsetState,
        _s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        Ok(())
    }
}

// A helper trait providing a 'subset' method for various subtables that have no associated tag
pub(crate) trait SubsetTable<'a> {
    type ArgsForSubset: 'a;
    type Output: 'a;
    /// Subset this table and write a subset version of this table into serializer
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags>;
}

// A helper trait providing a 'serialize' method
trait Serialize<'a> {
    type Args: 'a;
    /// Serialize this table
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags>;
}

pub fn subset_font<'a>(font: &FontRef<'a>, plan: &Plan) -> Result<Vec<u8>, SubsetError> {
    let mut builder = FontBuilder::<'a>::default();

    let mut state = SubsetState {
        passthrough_budget: font.data().len().saturating_mul(2),
        ..Default::default()
    };
    let mut tags_with_dependencies = Vec::with_capacity(5);
    let mut seen_tags = IntSet::<Tag>::empty();
    for record in font.table_directory().table_records() {
        let tag = record.tag();
        // Tables are looked up by tag, which finds the first record when a tag is
        // repeated, so further records would only subset the same table again.
        if !seen_tags.insert(tag) {
            continue;
        }
        if should_drop_table(tag, plan) {
            continue;
        }

        // CBDT is handled together with CBLC. Avoid allocating a serializer sized for the
        // (typically much larger) bitmap-data table when there is nothing to do here.
        if tag == Cbdt::TAG {
            continue;
        }

        // When glyph ids are unchanged, the bitmap location and data tables need no rewriting.
        // Borrow them directly until FontBuilder assembles the final font instead of copying the
        // bitmap data into an intermediate buffer first.
        if tag == Cblc::TAG {
            if let (Ok(cblc), Ok(cbdt)) = (font.cblc(), font.cbdt()) {
                if cblc::can_passthrough_bitmap_tables(&cblc, &cbdt, plan) {
                    builder.add_raw_with_checksum(
                        Cblc::TAG,
                        cblc.offset_data().as_bytes(),
                        record.checksum(),
                    );
                    if let Some(cbdt_record) = font
                        .table_directory()
                        .table_records()
                        .iter()
                        .find(|record| record.tag() == Cbdt::TAG)
                    {
                        builder.add_raw_with_checksum(
                            Cbdt::TAG,
                            cbdt.offset_data().as_bytes(),
                            cbdt_record.checksum(),
                        );
                    } else {
                        builder.add_raw(Cbdt::TAG, cbdt.offset_data().as_bytes());
                    }
                    continue;
                }
            }
        }

        // TODO: add more tags with dependencies for instancing
        match tag {
            Gpos::TAG => tags_with_dependencies.push(tag),
            _ => subset(tag, font, plan, &mut builder, &mut state)?,
        }
    }

    for tag in tags_with_dependencies {
        subset(tag, font, plan, &mut builder, &mut state)?;
    }
    Ok(builder.build())
}

fn should_drop_table(tag: Tag, plan: &Plan) -> bool {
    if plan.drop_tables.contains(tag) {
        return true;
    }

    let no_hinting = plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING);

    match tag {
        // hint tables
        Cvar::TAG | CVT | FPGM | PREP | Hdmx::TAG | VDMX => no_hinting,
        //TODO: drop var tables during instancing when all axes are pinned
        _ => false,
    }
}

fn subset<'a>(
    table_tag: Tag,
    font: &FontRef<'a>,
    plan: &Plan,
    builder: &mut FontBuilder<'a>,
    state: &mut SubsetState,
) -> Result<(), SubsetError> {
    // The CFF subsetter assembles a finished contiguous table, so adding it to
    // the font directly avoids copying it through the graph serializer. It also
    // avoids rerunning the complete CFF subset plan if an estimate is too small.
    if (table_tag == Cff::TAG || table_tag == Cff2::TAG)
        && !plan.no_subset_tables.contains(table_tag)
    {
        let data = cff::subset_data(font, plan, table_tag == Cff2::TAG)?;
        if !data.is_empty() {
            builder.add_raw(table_tag, data);
        }
        return Ok(());
    }

    let max_buf_size = max_subset_table_size(font, table_tag, plan);
    let buf_size = estimate_subset_table_size(font, table_tag, plan).min(max_buf_size);
    let mut s = Serializer::new(buf_size).map_err(|_| SubsetError::SubsetTableError(table_tag))?;
    let needed = try_subset(table_tag, font, plan, builder, &mut s, max_buf_size, state);
    if s.in_error() && !s.only_offset_overflow() {
        return Err(SubsetError::SubsetTableError(table_tag));
    }

    // table subsetted to empty
    if needed.is_err() {
        return Ok(());
    }

    //TODO: complete overflow resolution
    let subsetted_data = if !s.offset_overflow() {
        s.copy_bytes()
    } else {
        resolve_overflows(s, table_tag, 32).map_err(|_| SubsetError::SubsetTableError(table_tag))?
    };

    if !subsetted_data.is_empty() {
        builder.add_raw(table_tag, subsetted_data);
    }
    Ok(())
}

fn try_subset<'a>(
    table_tag: Tag,
    font: &FontRef<'a>,
    plan: &Plan,
    builder: &mut FontBuilder<'a>,
    s: &mut Serializer,
    max_buf_size: usize,
    state: &mut SubsetState,
) -> Result<(), SubsetError> {
    s.start_serialize()
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let ret = subset_table(table_tag, font, plan, builder, s, state);
    if !s.ran_out_of_room() {
        s.end_serialize();
        return ret;
    }

    // ran out of room, reallocate more bytes
    let Some(buf_size) = s
        .allocated()
        .checked_mul(2)
        .and_then(|size| size.checked_add(16))
    else {
        return ret;
    };
    if buf_size > max_buf_size {
        return ret;
    }
    s.reset_size(buf_size)
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;
    try_subset(table_tag, font, plan, builder, s, max_buf_size, state)
}

fn subset_table<'a>(
    tag: Tag,
    font: &FontRef<'a>,
    plan: &Plan,
    builder: &mut FontBuilder<'a>,
    s: &mut Serializer,
    state: &mut SubsetState,
) -> Result<(), SubsetError> {
    if plan.no_subset_tables.contains(tag) {
        return passthrough_table(tag, font, s, state);
    }

    match tag {
        Math::TAG => font
            .math()
            .map_err(|_| SubsetError::SubsetTableError(Math::TAG))?
            .subset(plan, font, s, builder),
        Base::TAG => font
            .base()
            .map_err(|_| SubsetError::SubsetTableError(Base::TAG))?
            .subset(plan, font, s, builder),

        //Skip, handled by Cblc
        Cbdt::TAG => Ok(()),

        Cblc::TAG => font
            .cblc()
            .map_err(|_| SubsetError::SubsetTableError(Cblc::TAG))?
            .subset(plan, font, s, builder),

        Cmap::TAG => font
            .cmap()
            .map_err(|_| SubsetError::SubsetTableError(Cmap::TAG))?
            .subset(plan, font, s, builder),

        Dmap::TAG => font
            .dmap()
            .map_err(|_| {
                s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                SubsetError::SubsetTableError(Dmap::TAG)
            })?
            .subset(plan, font, s, builder),

        Colr::TAG => font
            .colr()
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
            .subset(plan, font, s, builder),

        //TODO: if SVG is present and we support subsetting SVG table, pass through CPAL table
        // see fonttools: <https://github.com/fonttools/fonttools/blob/64e5277d040e1a5c84f21f8fb8a5dc7d8ad3c3fa/Lib/fontTools/subset/__init__.py#L2545>
        Cpal::TAG => font
            .cpal()
            .map_err(|_| SubsetError::SubsetTableError(Cpal::TAG))?
            .subset(plan, font, s, builder),

        Gdef::TAG => font
            .gdef()
            .map_err(|_| SubsetError::SubsetTableError(Gdef::TAG))?
            .subset_with_state(plan, font, state, s, builder),

        Glyf::TAG
            if font.data_for_tag(glyf_loca::GLYF).is_some()
                && !plan.drop_tables.contains(glyf_loca::GLYF) =>
        {
            Ok(())
        }

        Glyf::TAG => font
            .glyf()
            .map_err(|_| SubsetError::SubsetTableError(Glyf::TAG))?
            .subset(plan, font, s, builder),

        glyf_loca::GLYF => {
            let glyf = font.glyf_extended().map_err(|_| {
                s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                SubsetError::SubsetTableError(glyf_loca::GLYF)
            })?;
            glyf_loca::subset_glyf(&glyf, plan, font, s, builder, true)
        }

        Gpos::TAG => font
            .gpos()
            .map_err(|_| SubsetError::SubsetTableError(Gpos::TAG))?
            .subset_with_state(plan, font, state, s, builder),

        Gsub::TAG => font
            .gsub()
            .map_err(|_| SubsetError::SubsetTableError(Gsub::TAG))?
            .subset_with_state(plan, font, state, s, builder),

        Gvar::TAG
            if font.data_for_tag(GvarExtended::TAG).is_some()
                && !plan.drop_tables.contains(GvarExtended::TAG) =>
        {
            Ok(())
        }

        Gvar::TAG => font
            .gvar()
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?
            .subset(plan, font, s, builder),

        GvarExtended::TAG => font
            .gvar_extended()
            .map_err(|_| {
                s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                SubsetError::SubsetTableError(GvarExtended::TAG)
            })?
            .subset(plan, font, s, builder),

        Hdmx::TAG => font
            .hdmx()
            .map_err(|_| SubsetError::SubsetTableError(Hdmx::TAG))?
            .subset(plan, font, s, builder),

        //handled by glyf table if exists
        Head::TAG => font.selected_glyf_loca(None).map(|_| ()).or_else(|_| {
            font.head()
                .map_err(|_| SubsetError::SubsetTableError(Head::TAG))?
                .subset(plan, font, s, builder)
        }),

        //Skip, handled by Hmtx
        Hhea::TAG | HheaExtended::TAG => Ok(()),

        Hmtx::TAG
            if font.data_for_tag(HmtxExtended::TAG).is_some()
                && !plan.drop_tables.contains(HmtxExtended::TAG) =>
        {
            Ok(())
        }

        Hmtx::TAG => font
            .hmtx()
            .map_err(|_| SubsetError::SubsetTableError(Hmtx::TAG))?
            .subset(plan, font, s, builder),

        HmtxExtended::TAG => font
            .hmtx_extended()
            .map_err(|_| {
                s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                SubsetError::SubsetTableError(HmtxExtended::TAG)
            })?
            .subset(plan, font, s, builder),

        //Skip, handled by Vmtx
        Vhea::TAG | VheaExtended::TAG => Ok(()),

        Vmtx::TAG
            if font.data_for_tag(VmtxExtended::TAG).is_some()
                && !plan.drop_tables.contains(VmtxExtended::TAG) =>
        {
            Ok(())
        }

        Vmtx::TAG => font
            .vmtx()
            .map_err(|_| SubsetError::SubsetTableError(Vmtx::TAG))?
            .subset(plan, font, s, builder),

        VmtxExtended::TAG => font
            .vmtx_extended()
            .map_err(|_| {
                s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                SubsetError::SubsetTableError(VmtxExtended::TAG)
            })?
            .subset(plan, font, s, builder),

        Hvar::TAG => font
            .hvar()
            .map_err(|_| SubsetError::SubsetTableError(Hvar::TAG))?
            .subset(plan, font, s, builder),

        Vvar::TAG => font
            .vvar()
            .map_err(|_| SubsetError::SubsetTableError(Vvar::TAG))?
            .subset(plan, font, s, builder),

        Fvar::TAG | Cvar::TAG | Avar::TAG => passthrough_table(tag, font, s, state),
        // MVAR/CVT: not supported by read-fonts, but we want to pass through it during subsetting
        MVAR | CVT => passthrough_table(tag, font, s, state),

        //Skip, handled by glyf
        Loca::TAG | glyf_loca::LOCA => Ok(()),

        // Hybrid output is not maintained: MAXP describes the retained glyph space.
        Maxp::TAG
            if font.data_for_tag(MaxpExtended::TAG).is_some()
                && !plan.drop_tables.contains(MaxpExtended::TAG) =>
        {
            Ok(())
        }

        Maxp::TAG => font
            .maxp()
            .map_err(|_| SubsetError::SubsetTableError(Maxp::TAG))?
            .subset(plan, font, s, builder),

        MaxpExtended::TAG => font
            .maxp_extended()
            .map_err(|_| SubsetError::SubsetTableError(MaxpExtended::TAG))?
            .subset(plan, font, s, builder),

        Name::TAG => font
            .name()
            .map_err(|_| SubsetError::SubsetTableError(Name::TAG))?
            .subset(plan, font, s, builder),

        Os2::TAG => font
            .os2()
            .map_err(|_| SubsetError::SubsetTableError(Os2::TAG))?
            .subset(plan, font, s, builder),

        Post::TAG => font
            .post()
            .map_err(|_| SubsetError::SubsetTableError(Post::TAG))?
            .subset(plan, font, s, builder),

        Sbix::TAG => font
            .sbix()
            .map_err(|_| SubsetError::SubsetTableError(Sbix::TAG))?
            .subset(plan, font, s, builder),

        Vorg::TAG => font
            .vorg()
            .map_err(|_| SubsetError::SubsetTableError(Vorg::TAG))?
            .subset(plan, font, s, builder),

        Stat::TAG => passthrough_table(tag, font, s, state),

        _ if plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED) =>
        {
            passthrough_table(tag, font, s, state)
        }
        _ => Ok(()),
    }
}

/// Copies a table to the output unmodified.
///
/// Every table is copied separately, and nothing stops the records of different
/// tables from sharing the same bytes, so the copies are charged to a budget of
/// twice the size of the font. Tables that don't overlap can't add up to more than
/// the font; the rest is room for tables that do share bytes.
fn passthrough_table(
    tag: Tag,
    font: &FontRef<'_>,
    s: &mut Serializer,
    state: &mut SubsetState,
) -> Result<(), SubsetError> {
    if let Some(data) = font.data_for_tag(tag) {
        let len = data.len();
        if len > state.passthrough_budget {
            // fail the subset rather than drop the table as if it subset to empty
            s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            return Err(SubsetError::SubsetTableError(tag));
        }
        s.embed_bytes(data.as_bytes())
            .map_err(|_| SubsetError::SubsetTableError(tag))?;
        // charged only once embedded: try_subset retries a table that ran out of room
        state.passthrough_budget -= len;
    }
    Ok(())
}

pub fn estimate_subset_table_size(font: &FontRef, table_tag: Tag, plan: &Plan) -> usize {
    let Some(table_data) = font.data_for_tag(table_tag) else {
        return 0;
    };

    let table_len = table_data.len();
    let mut bulk: usize = 8192;
    if table_tag == Name::TAG {
        // Overrides can be much larger than the source name table, so make room for
        // them up front instead of growing the buffer in another subset pass.
        bulk += name_overrides_len(plan);
    }
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
            // Add some extra room for the CFF charset.
            bulk += src_glyphs * 16;
        } else if table_tag == Cff2::TAG {
            // Just extra CharString offsets.
            bulk += src_glyphs * 4;
        }
    }

    if src_glyphs == 0 || same_size {
        return bulk + table_len;
    }

    // glyf: the retained glyphs' loca lengths, rounded up to even, are a cheap upper
    // bound of the output size, since glyphs are only ever trimmed and a short loca
    // output pads each glyph to an even length. The glyph count ratio below can
    // underestimate badly when the dropped glyphs are small, which forces a second,
    // larger subset pass.
    if table_tag == Glyf::TAG {
        if let Some(len) = retained_glyf_len(font, plan, table_len) {
            return bulk + len;
        }
    }

    bulk + (table_len as f32 * (dst_glyphs as f32 / src_glyphs as f32).sqrt()) as usize
}

/// The largest serializer buffer, in bytes, that subsetting one table may allocate.
///
/// The buffer starts at the size estimate and grows whenever the table runs out of
/// room, so without a bound a malformed font could make it grow until allocation
/// fails. A table's subset shouldn't need more than twice the size of the whole
/// font file, plus, for name, the records the caller added with overrides, which
/// don't come from the font.
///
/// If the bound doesn't fit in `usize`, there is no bound beyond what can be
/// allocated.
fn max_subset_table_size(font: &FontRef, table_tag: Tag, plan: &Plan) -> usize {
    let max = font.data().len().checked_mul(2);
    let max = if table_tag == Name::TAG {
        max.and_then(|max| max.checked_add(name_overrides_len(plan)))
    } else {
        max
    };
    max.unwrap_or(usize::MAX)
}

/// Bytes needed for the name records the caller added with overrides.
fn name_overrides_len(plan: &Plan) -> usize {
    plan.name_table_overrides
        .values()
        .map(|bytes| bytes.len() + name::NAME_RECORD_SIZE)
        .sum()
}

/// Sum of the source glyf lengths of all retained glyphs according to loca, each
/// rounded up to even, bounded by the length of glyf.
///
/// loca is untrusted, and the output buffer is allocated from this estimate before
/// any glyph is read, so loca must not be able to choose the allocation size:
/// - a glyph whose range is reversed or ends past `glyf_len` counts as empty; the
///   glyf subsetter rejects it later with an error;
/// - the total is capped at `glyf_len` plus one padding byte per retained glyph,
///   so in-bounds but overlapping ranges cannot inflate it either.
///
/// A font with overlapping glyphs can legitimately need more room than the cap;
/// `try_subset` then grows the buffer, as it does for any underestimate, up to
/// [`max_subset_table_size`].
fn retained_glyf_len(font: &FontRef, plan: &Plan, glyf_len: usize) -> Option<usize> {
    let loca = font.loca(None).ok()?;
    let total: usize = plan
        .new_to_old_gid_list
        .iter()
        .map(|(_, old_gid)| {
            let gid = old_gid.to_u32() as usize;
            match (loca.get_raw(gid), loca.get_raw(gid + 1)) {
                (Some(start), Some(end)) if start <= end && end as usize <= glyf_len => {
                    let len = (end - start) as usize;
                    len + len % 2
                }
                _ => 0,
            }
        })
        // each term is at most glyf_len + 1, so this cannot overflow in practice;
        // saturate anyway rather than rely on that
        .fold(0usize, usize::saturating_add);
    Some(total.min(glyf_len.saturating_add(plan.new_to_old_gid_list.len())))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_varstore_inner_maps() {
        // minors of the majors 0 and 2; major 5 is past the 4 VarData tables
        let mut varidx_set = IntSet::empty();
        varidx_set.extend([
            0x0_0009_u32,
            0x0_0005,
            0x2_0007,
            0x2_0001,
            0x2_0003,
            0x5_0000,
        ]);

        let mut inner_maps = Vec::new();
        generate_varstore_inner_maps(&varidx_set, &[10, 10, 10, 10], &mut inner_maps);
        let minors: Vec<Vec<u32>> = inner_maps
            .iter()
            .map(|m| m.keys().copied().collect())
            .collect();
        assert_eq!(minors, [vec![5, 9], vec![], vec![1, 3, 7], vec![]]);
        // the new minors follow the order of the old ones
        assert_eq!(inner_maps[2].get(3), Some(&1));
    }

    /// An index past the delta sets of its subtable is not a row to retain,
    /// and is remapped to no variation without taking a new row.
    #[test]
    fn variation_indices_without_a_delta_set_are_not_retained() {
        // the subtables have 6, 0 and 4 delta sets; major 3 is past them
        let delta_set_counts = [6, 0, 4];
        let mut varidx_set = IntSet::empty();
        varidx_set.extend([
            0x0_0005_u32,
            0x0_0006,
            0x0_ffff,
            0x1_0000,
            0x2_0001,
            0x2_0003,
            0x2_0004,
            0x3_0000,
        ]);

        let mut inner_maps = Vec::new();
        generate_varstore_inner_maps(&varidx_set, &delta_set_counts, &mut inner_maps);
        let minors: Vec<Vec<u32>> = inner_maps
            .iter()
            .map(|m| m.keys().copied().collect())
            .collect();
        assert_eq!(minors, [vec![5], vec![], vec![1, 3]]);

        let mut varidx_delta_map = FastHashMap::default();
        remap_variation_indices(&delta_set_counts, &varidx_set, &mut varidx_delta_map);
        let new_idx = |var_idx: u32| varidx_delta_map.get(&var_idx).map(|(idx, _)| *idx);
        assert_eq!(new_idx(0x0_0005), Some(0x0_0000));
        assert_eq!(new_idx(0x0_0006), Some(NO_VARIATION_INDEX));
        assert_eq!(new_idx(0x0_ffff), Some(NO_VARIATION_INDEX));
        // a subtable without a retained row is not written, and takes no new major
        assert_eq!(new_idx(0x1_0000), Some(NO_VARIATION_INDEX));
        assert_eq!(new_idx(0x2_0001), Some(0x1_0000));
        assert_eq!(new_idx(0x2_0003), Some(0x1_0001));
        assert_eq!(new_idx(0x2_0004), Some(NO_VARIATION_INDEX));
        assert_eq!(new_idx(0x3_0000), None);
    }

    /// A store with a subtable that can't be read fails its table, as a table
    /// that HarfBuzz's sanitizer rejects does there.
    #[test]
    fn variation_store_with_an_unreadable_subtable() {
        use font_test_data::bebuffer::BeBuffer;
        use read_fonts::{FontData, FontRead};

        // one region; three subtable offsets: a subtable of two delta sets, a
        // null offset, and `last_offset`
        let store = |last_offset: u32| {
            BeBuffer::new()
                .push(1_u16)
                .push(20_u32)
                .push(3_u16)
                .extend([30_u32, 0, last_offset])
                .extend([1_u16, 1])
                .extend([0_i16, 0x4000, 0x4000])
                .extend([2_u16, 0, 1, 0])
                .extend([7_u8, 8])
                .to_vec()
        };
        let counts = |bytes: &[u8]| {
            let store = ItemVariationStore::read(FontData::new(bytes)).unwrap();
            delta_set_counts(&store)
        };
        assert_eq!(counts(&store(30)).unwrap(), [2, 0, 2]);
        // the offset leads past the table
        assert!(counts(&store(0x00FF_FFFF)).is_err());

        // GDEF 1.3 with nothing but such a store
        let mut gdef = BeBuffer::new()
            .extend([1_u16, 3, 0, 0, 0, 0, 0])
            .push(18_u32)
            .to_vec();
        gdef.extend(store(0x00FF_FFFF));
        let mut builder = FontBuilder::new();
        builder.add_raw(Tag::new(b"maxp"), vec![0, 0, 0x50, 0, 0, 3]);
        builder.add_raw(Gdef::TAG, gdef);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let plan = Plan::new(
            &(0..3_u32).map(GlyphId::new).collect(),
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        assert!(plan.layout_ops_exceeded.contains(Gdef::TAG));
        assert!(matches!(
            subset_font(&font, &plan),
            Err(SubsetError::SubsetTableError(Gdef::TAG))
        ));
    }

    /// GPOS names two delta sets of a GDEF store that has one: the second is
    /// not retained, and its device table is dropped.
    #[test]
    fn layout_variation_index_without_a_delta_set() {
        use font_test_data::bebuffer::BeBuffer;
        use read_fonts::tables::{
            gpos::{PositionSubtables, SinglePos},
            layout::DeviceOrVariationIndex,
        };

        // GDEF 1.3 with a store of one region and one ItemVariationData: one
        // delta set, of delta 7
        let gdef = BeBuffer::new()
            .extend([1_u16, 3, 0, 0, 0, 0, 0])
            .push(18_u32)
            .push(1_u16)
            .push(12_u32)
            .push(1_u16)
            .push(22_u32)
            .extend([1_u16, 1])
            .extend([0_i16, 0x4000, 0x4000])
            .extend([1_u16, 0, 1, 0])
            .push(7_u8)
            .to_vec();
        // GPOS with one SinglePosFormat2 lookup: glyph 1 has the device of delta
        // set (0, 0), glyph 2 that of (0, 5)
        let gpos = BeBuffer::new()
            .extend([1_u16, 0, 10, 30, 44])
            // ScriptList
            .push(1_u16)
            .push(Tag::new(b"DFLT"))
            .extend([8_u16, 4, 0, 0, 0xFFFF, 1, 0])
            // FeatureList
            .push(1_u16)
            .push(Tag::new(b"kern"))
            .extend([8_u16, 0, 1, 0])
            // LookupList, Lookup
            .extend([1_u16, 4, 1, 0, 1, 8])
            // SinglePosFormat2: coverage at 28, x advance and its device
            .extend([2_u16, 28, 0x0044, 2])
            .extend([10_u16, 16, 20, 22])
            .extend([0_u16, 0, 0x8000])
            .extend([0_u16, 5, 0x8000])
            // Coverage
            .extend([1_u16, 2, 1, 2])
            .to_vec();
        let mut builder = FontBuilder::new();
        builder.add_raw(Tag::new(b"maxp"), vec![0, 0, 0x50, 0, 0, 3]);
        builder.add_raw(Gdef::TAG, gdef);
        builder.add_raw(Gpos::TAG, gpos);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();

        let plan = Plan::new(
            &(0..3_u32).map(GlyphId::new).collect(),
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        assert_eq!(plan.layout_varidx_delta_map.get(&0), Some(&(0, 0)));
        assert_eq!(
            plan.layout_varidx_delta_map.get(&5),
            Some(&(NO_VARIATION_INDEX, 0))
        );
        assert_eq!(plan.gdef_varstore_inner_maps.len(), 1);
        assert_eq!(
            plan.gdef_varstore_inner_maps[0]
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [0]
        );

        let out = subset_font(&font, &plan).unwrap();
        let out = FontRef::new(&out).unwrap();
        let store = out.gdef().unwrap().item_var_store().unwrap().unwrap();
        let var_data = store.item_variation_data().get(0).unwrap().unwrap();
        assert_eq!(var_data.item_count(), 1);
        assert_eq!(var_data.delta_sets(), [7]);

        let lookup = out.gpos().unwrap().lookup_list().unwrap().lookups().get(0);
        let PositionSubtables::Single(subtables) = lookup.unwrap().subtables().unwrap() else {
            panic!("not a SinglePos lookup");
        };
        let SinglePos::Format2(single_pos) = subtables.get(0).unwrap() else {
            panic!("not SinglePosFormat2");
        };
        let records: Vec<_> = single_pos
            .value_records()
            .iter()
            .map(|record| record.unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].x_advance(), Some(10));
        assert_eq!(records[1].x_advance(), Some(20));
        // the device of glyph 1 names the row that is left
        let Some(Ok(DeviceOrVariationIndex::VariationIndex(device))) =
            records[0].x_advance_device()
        else {
            panic!("no variation index for glyph 1");
        };
        assert_eq!(
            (
                device.delta_set_outer_index(),
                device.delta_set_inner_index()
            ),
            (0, 0)
        );
        assert!(records[1].x_advance_device().is_none());
    }

    /// A table's serializer buffer is bounded by twice the font file size, plus
    /// the caller's overrides for name.
    #[test]
    fn max_subset_table_size_is_twice_the_font() {
        let bytes = std::fs::read("test-data/fonts/Roboto-Regular.ttf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut plan = Plan::new(
            &IntSet::empty(),
            &parse_unicodes("*").unwrap(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        let twice_font = bytes.len() * 2;
        assert_eq!(max_subset_table_size(&font, Glyf::TAG, &plan), twice_font);
        assert_eq!(max_subset_table_size(&font, Name::TAG, &plan), twice_font);

        plan.override_name_table(NameId::new(1), 3, 1, 0x409, Some(&"x".repeat(1000)))
            .unwrap();
        let overrides = name_overrides_len(&plan);
        assert!(overrides > 1000);
        assert_eq!(
            max_subset_table_size(&font, Name::TAG, &plan),
            twice_font + overrides
        );
        // overrides only add room to name
        assert_eq!(max_subset_table_size(&font, Glyf::TAG, &plan), twice_font);
    }

    /// The glyf buffer estimate must cover the subset glyf table, so glyf is
    /// subset in a single pass even when the dropped glyphs are small.
    #[test]
    fn glyf_size_estimate_covers_output() {
        for (path, unicodes) in [
            ("test-data/fonts/Roboto-Regular.ttf", "*"),
            ("test-data/fonts/Roboto-Regular.ttf", "41-5A,61-7A"),
            ("test-data/fonts/Amiri-Regular.ttf", "*"),
            ("test-data/fonts/Amiri-Regular.ttf", "600-6FF"),
        ] {
            let bytes = std::fs::read(path).unwrap();
            let font = FontRef::new(&bytes).unwrap();
            let plan = Plan::new(
                &IntSet::empty(),
                &parse_unicodes(unicodes).unwrap(),
                &font,
                SubsetFlags::default(),
                &IntSet::empty(),
                &IntSet::all(),
                &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
                &IntSet::empty(),
                &IntSet::empty(),
            );
            let estimate = estimate_subset_table_size(&font, Glyf::TAG, &plan);
            let out = subset_font(&font, &plan).unwrap();
            let glyf_len = FontRef::new(&out)
                .unwrap()
                .glyf()
                .unwrap()
                .offset_data()
                .len();
            // the estimate includes 8192 bytes of slack
            assert!(
                estimate - 8192 >= glyf_len,
                "{path} {unicodes}: estimate {estimate}, glyf {glyf_len}"
            );
        }
    }

    /// A tiny glyf font whose long loca is `0, odd, 0, odd, ...`: every even glyph
    /// claims `odd` bytes of a 4-byte glyf, every odd glyph ends before it starts.
    fn font_with_hostile_loca(num_glyphs: u16, odd: u32) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        // indexToLocFormat: long offsets
        head[50..52].copy_from_slice(&1i16.to_be_bytes());
        // maxp version 0.5
        let mut maxp = 0x0000_5000u32.to_be_bytes().to_vec();
        maxp.extend(num_glyphs.to_be_bytes());
        let loca: Vec<u8> = (0..=num_glyphs as u32)
            .flat_map(|i| (if i % 2 == 1 { odd } else { 0 }).to_be_bytes())
            .collect();
        FontBuilder::new()
            .add_raw(Head::TAG, head)
            .add_raw(Maxp::TAG, maxp)
            .add_raw(Loca::TAG, loca)
            .add_raw(Glyf::TAG, vec![0u8; 4])
            .build()
    }

    /// loca must not choose the size of the glyf buffer, which is allocated before
    /// any glyph is read: a few hundred bytes of loca could otherwise request
    /// ~120 GiB and abort the process in `Serializer::new` (ANT-2026-7O19I20P).
    ///
    /// The estimate is asserted before subsetting, so a regression fails this test
    /// instead of attempting the huge allocation.
    #[test]
    fn glyf_size_estimate_ignores_hostile_loca() {
        const NUM_GLYPHS: u16 = 60;
        const GLYF_LEN: usize = 4;
        // 0xFFFFFFFE: even glyphs reach far past the end of glyf.
        // GLYF_LEN: even glyphs stay inside glyf but all overlap.
        for odd in [0xFFFF_FFFE, GLYF_LEN as u32] {
            let bytes = font_with_hostile_loca(NUM_GLYPHS, odd);
            let font = FontRef::new(&bytes).unwrap();
            let gids: IntSet<GlyphId> = (0..NUM_GLYPHS as u32).map(GlyphId::new).collect();
            let plan = Plan::new(
                &gids,
                &IntSet::empty(),
                &font,
                SubsetFlags::default(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
            );
            let estimate = estimate_subset_table_size(&font, Glyf::TAG, &plan);
            // 8192 bytes of fixed slack, then at most glyf plus one pad byte per glyph
            let bound = 8192 + GLYF_LEN + NUM_GLYPHS as usize;
            assert!(
                estimate <= bound,
                "loca {odd:#x}: estimate {estimate} exceeds {bound}"
            );
            // the odd glyphs are malformed, so subsetting reports an error
            assert!(
                subset_font(&font, &plan).is_err(),
                "loca {odd:#x}: malformed glyphs were accepted"
            );
        }
    }

    #[test]
    fn requested_glyph_map() {
        let bytes = std::fs::read("test-data/fonts/Roboto-Regular.ttf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let new_plan = |gids: &[u32], flags, glyph_map: &[(u32, u32)]| {
            let glyph_map: FastHashMap<_, _> = glyph_map
                .iter()
                .map(|(old, new)| (GlyphId::new(*old), GlyphId::new(*new)))
                .collect();
            Plan::new_with_glyph_map(
                &gids.iter().copied().map(GlyphId::new).collect(),
                &IntSet::empty(),
                &font,
                flags,
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &glyph_map,
            )
        };

        // 0 always stays 0. Glyphs missing from the map follow the highest requested new glyph.
        let plan = new_plan(
            &[37, 39, 69, 74],
            SubsetFlags::default(),
            &[(0, 5), (69, 2), (37, 7)],
        )
        .unwrap();
        let pairs: Vec<_> = plan
            .new_to_old_gid_list
            .iter()
            .map(|(new, old)| (new.to_u32(), old.to_u32()))
            .collect();
        assert_eq!(pairs, [(0, 0), (2, 69), (7, 37), (8, 39), (9, 74)]);
        assert_eq!(plan.num_output_glyphs, 10);
        assert_eq!(
            plan.glyph_map.get(&GlyphId::new(37)),
            Some(&GlyphId::new(7))
        );
        assert_eq!(
            plan.reverse_glyph_map.get(&GlyphId::new(8)),
            Some(&GlyphId::new(39))
        );

        assert!(matches!(
            new_plan(&[37, 39], SubsetFlags::default(), &[(37, 1), (39, 1)]),
            Err(SubsetError::NonUniqueGlyphMap)
        ));
        assert!(matches!(
            new_plan(&[37], SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS, &[(37, 1)]),
            Err(SubsetError::GlyphMapWithRetainGids)
        ));
        // An empty map is the same as Plan::new.
        assert!(new_plan(&[37], SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS, &[]).is_ok());
    }

    #[test]
    fn configurable_passthrough_copies_an_unrecognized_table() {
        let original = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let mut builder = FontBuilder::new();
        for record in original.table_directory().table_records() {
            builder.add_raw(record.tag(), original.data_for_tag(record.tag()).unwrap());
        }
        let tag = Tag::new(b"TEST");
        builder.add_raw(tag, b"opaque table".to_vec());
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let mut plan = Plan::new(
            &IntSet::empty(),
            &IntSet::empty(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        let before = subset_font(&font, &plan).unwrap();
        assert!(FontRef::new(&before).unwrap().data_for_tag(tag).is_none());
        plan.set_no_subset_tables(&[tag].into_iter().collect());
        let after = subset_font(&font, &plan).unwrap();
        assert_eq!(
            FontRef::new(&after)
                .unwrap()
                .data_for_tag(tag)
                .unwrap()
                .as_bytes(),
            b"opaque table"
        );
    }

    #[test]
    fn public_plan_mappings_include_closure_and_original_unicode_glyphs() {
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let unicodes = [0x2C, 0x31].into_iter().collect();
        let glyphs = IntSet::empty();
        for flags in [
            SubsetFlags::default(),
            SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS,
        ] {
            let mut plan = Plan::new(
                &glyphs,
                &unicodes,
                &font,
                flags,
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
                &IntSet::empty(),
            );
            let mapping: std::collections::BTreeMap<_, _> =
                plan.old_to_new_glyph_mapping().collect();
            assert!(mapping.contains_key(&GlyphId::new(0)));
            assert!(mapping.contains_key(&GlyphId::new(2)));
            assert!(mapping.contains_key(&GlyphId::new(4)));
            let unicode_mapping: std::collections::BTreeMap<_, _> =
                plan.unicode_to_old_glyph_mapping().collect();
            assert_eq!(unicode_mapping[&0x2C], GlyphId::new(2));
            assert_eq!(unicode_mapping[&0x31], GlyphId::new(4));
            if flags.contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS) {
                assert!(mapping.iter().all(|(old, new)| old == new));
            }
            let no_subset = [Tag::new(b"name")].into_iter().collect();
            plan.set_no_subset_tables(&no_subset);
            assert_eq!(plan.no_subset_tables, no_subset);
        }
    }

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
    }

    #[test]
    fn glyf_closure_composite_glyphs() {
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let mut gids = IntSet::empty();

        let mut bytes_budget = glyf.offset_data().len() as i64;
        glyf_closure_glyphs(
            &loca,
            &glyf,
            GlyphId::new(5),
            &mut gids,
            64,
            &mut bytes_budget,
            0,
        );
        assert_eq!(gids.len(), 2);
        assert!(gids.contains(GlyphId::new(5)));
        assert!(gids.contains(GlyphId::new(1)));
    }

    #[test]
    fn glyf_closure_charges_glyph_bytes() {
        let font = FontRef::new(font_test_data::GLYF_COMPONENTS).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let glyph_len = |gid: u32| {
            let glyph = loca.get(GlyphId::new(gid), &glyf).unwrap();
            glyph.into_glyph().unwrap().offset_data().len() as i64
        };

        // the composite glyph 5 and its component, glyph 1, are both charged
        let budget = glyph_len(5) + glyph_len(1);
        let mut gids = IntSet::empty();
        let mut bytes_budget = budget;
        glyf_closure_glyphs(
            &loca,
            &glyf,
            GlyphId::new(5),
            &mut gids,
            64,
            &mut bytes_budget,
            0,
        );
        assert_eq!(bytes_budget, 0);
        assert_eq!(gids.len(), 2);

        // one byte less, and the budget is exceeded
        let mut gids = IntSet::empty();
        let mut bytes_budget = budget - 1;
        glyf_closure_glyphs(
            &loca,
            &glyf,
            GlyphId::new(5),
            &mut gids,
            64,
            &mut bytes_budget,
            0,
        );
        assert!(bytes_budget < 0);

        // a composite that exceeds the budget is retained but not expanded
        let mut gids = IntSet::empty();
        let mut bytes_budget = glyph_len(5) - 1;
        glyf_closure_glyphs(
            &loca,
            &glyf,
            GlyphId::new(5),
            &mut gids,
            64,
            &mut bytes_budget,
            0,
        );
        assert!(bytes_budget < 0);
        assert_eq!(gids.len(), 1);
        assert!(gids.contains(GlyphId::new(5)));
    }

    /// Builds a font with a maxp and `tags.len()` unrecognized tables of 1 KiB, whose
    /// records either cover disjoint bytes or all share the same bytes.
    fn font_with_unrecognized_tables(tags: &[[u8; 4]], overlapping: bool) -> Vec<u8> {
        const TABLE_LEN: u32 = 1024;
        let num_tables = tags.len() as u16 + 1;
        let mut bytes = vec![0, 1, 0, 0];
        bytes.extend(num_tables.to_be_bytes());
        // searchRange, entrySelector, rangeShift are not used
        bytes.extend([0; 6]);
        let maxp_offset = 12 + 16 * num_tables as u32;
        let data_offset = maxp_offset + 8;
        let mut records: Vec<([u8; 4], u32, u32)> = tags
            .iter()
            .enumerate()
            .map(|(i, tag)| {
                let i = if overlapping { 0 } else { i as u32 };
                (*tag, data_offset + i * TABLE_LEN, TABLE_LEN)
            })
            .collect();
        records.push((*b"maxp", maxp_offset, 6));
        records.sort();
        for (tag, offset, len) in records {
            bytes.extend(tag);
            bytes.extend(0_u32.to_be_bytes());
            bytes.extend(offset.to_be_bytes());
            bytes.extend(len.to_be_bytes());
        }
        // maxp version 0.5 with 1 glyph, padded to 8 bytes
        bytes.extend([0, 0, 0x50, 0, 0, 1, 0, 0]);
        let num_copies = if overlapping { 1 } else { tags.len() };
        bytes.extend((0..TABLE_LEN * num_copies as u32).map(|i| i as u8));
        bytes
    }

    fn subset_keeping_unrecognized_tables(bytes: &[u8]) -> Result<Vec<u8>, SubsetError> {
        let font = FontRef::new(bytes).unwrap();
        let plan = Plan::new(
            &IntSet::empty(),
            &IntSet::empty(),
            &font,
            SubsetFlags::SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED,
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        subset_font(&font, &plan)
    }

    #[test]
    fn passthrough_tables_are_limited_by_font_size() {
        let tags: Vec<[u8; 4]> = (0..8_u8).map(|i| [b'X', b'0', b'0', b'0' + i]).collect();

        let disjoint = font_with_unrecognized_tables(&tags, false);
        let out = subset_keeping_unrecognized_tables(&disjoint).unwrap();
        let (font, out) = (
            FontRef::new(&disjoint).unwrap(),
            FontRef::new(&out).unwrap(),
        );
        for tag in tags.iter().map(Tag::new) {
            let data = |font: &FontRef| font.table_data(tag).map(|data| data.as_bytes().to_vec());
            assert_eq!(data(&out), data(&font));
        }

        // a single table that shares nothing is still copied
        let single = font_with_unrecognized_tables(&tags[..1], true);
        assert!(subset_keeping_unrecognized_tables(&single).is_ok());

        // two records sharing the same KiB fit in twice the size of the font
        let shared = font_with_unrecognized_tables(&tags[..2], true);
        let out = subset_keeping_unrecognized_tables(&shared).unwrap();
        assert!(out.len() > shared.len());

        // three no longer do, nor do eight, which would copy it eight times
        for num_records in [3, 8] {
            let overlapping = font_with_unrecognized_tables(&tags[..num_records], true);
            assert!(subset_keeping_unrecognized_tables(&overlapping).is_err());
        }
    }

    /// Every record of a repeated tag resolves to the same table, which is
    /// subset (here: copied, and charged to the passthrough budget) only once.
    #[test]
    fn repeated_table_tags_are_subset_once() {
        let tags = [*b"X000"; 8];
        let font = font_with_unrecognized_tables(&tags, true);
        let out = subset_keeping_unrecognized_tables(&font).unwrap();
        let out = FontRef::new(&out).unwrap();
        let records = out.table_directory().table_records();
        assert_eq!(
            records
                .iter()
                .filter(|r| r.tag() == Tag::new(b"X000"))
                .count(),
            1
        );
        let font = FontRef::new(&font).unwrap();
        let data = |font: &FontRef| {
            font.table_data(Tag::new(b"X000"))
                .unwrap()
                .as_bytes()
                .to_vec()
        };
        assert_eq!(data(&out), data(&font));
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

    /// A font without a maxp table used to panic when its plan was created.
    #[test]
    fn font_without_maxp_has_no_glyphs() {
        let font = FontRef::new(font_test_data::NAMES_ONLY).unwrap();
        assert!(font.maxp().is_err());
        let plan = Plan::new(
            &IntSet::empty(),
            &parse_unicodes("*").unwrap(),
            &font,
            SubsetFlags::default(),
            &IntSet::empty(),
            &IntSet::all(),
            &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
            &IntSet::empty(),
            &IntSet::empty(),
        );
        assert_eq!(plan.font_num_glyphs, 0);
        assert!(plan.glyphset.is_empty());
    }
}
