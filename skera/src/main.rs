//! binary subset tool
//!
//! Takes a font file and a subset input which describes the desired subset, and output is a new
//! font file containing only the data specified in the input.
//!

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use read_fonts::{
    collections::{int_set::Domain, IntSet},
    tables::{ebdt, eblc, feat, svg},
    types::{GlyphId, NameId, Tag},
    FontRef, TopLevelTable,
};
use skera::{
    parse_name_ids, parse_name_languages, parse_tag_list, parse_unicodes, populate_gids,
    subset_font, Plan, SubsetError, SubsetFlags, DEFAULT_LAYOUT_FEATURES, DSIG, EBSC, GLAT, GLOC,
    JSTF, KERN, KERX, LTSH, MORT, MORX, PCLT, SILF, SILL,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input font file.
    #[arg(short, long, visible_alias = "font-file")]
    path: std::path::PathBuf,

    /// The output font file
    #[arg(short, long)]
    output_file: std::path::PathBuf,

    #[command(flatten)]
    lists: ListArgs,

    /// drop hints
    #[arg(long)]
    no_hinting: bool,

    /// If set don't renumber glyph ids in the subset.
    #[arg(long)]
    retain_gids: bool,

    /// Remove CFF/CFF2 use of subroutines
    #[arg(long)]
    desubroutinize: bool,

    /// Keep legacy (non-Unicode) 'name' table entries
    #[arg(long)]
    name_legacy: bool,

    /// Set the overlaps flag on each glyph
    #[arg(long)]
    set_overlaps_flag: bool,

    /// Keep the outline of .notdef glyph
    #[arg(long)]
    notdef_outline: bool,

    /// Don't change the 'OS/2 ulUnicodeRange*' bits
    #[arg(long)]
    no_prune_unicode_ranges: bool,

    /// Don't perform glyph closure for layout substitution (GSUB)
    #[arg(long)]
    no_layout_closure: bool,

    /// Don't perform glyph closure for bidi mirrored variants of input glyphs
    #[arg(long)]
    no_bidi_closure: bool,

    /// Keep PS glyph names in TT-flavored fonts
    #[arg(long)]
    glyph_names: bool,

    /// Do not drop tables that the tool does not know how to subset
    #[arg(long)]
    passthrough_tables: bool,

    /// When retain gids is set also don't change the number of glyphs in the input font
    #[arg(long)]
    retain_num_glyphs: bool,

    /// Perform IUP delta optimization on the resulting gvar table's deltas
    #[arg(long)]
    optimize: bool,

    ///run subsetter N times
    #[arg(short, long)]
    num_iterations: Option<u32>,
}

/// Options that edit a set, like in hb-subset: `--name=` replaces the set, `--name+=` adds to it
/// and `--name-=` removes from it. They may be repeated and are applied in command line order,
/// so the values are read from [`ArgMatches`] by [`apply_list_args`] rather than from these fields.
#[derive(clap::Args, Debug)]
//Allow name_IDs, so we keep the option name consistent with HB and fonttools
#[allow(non_snake_case, dead_code)]
struct ListArgs {
    /// List of glyph ids. Use --gids+=... or --gids-=... to add to or remove from the set
    #[arg(short, long)]
    gids: Vec<String>,
    #[arg(long = "gids+", hide = true)]
    gids_add: Vec<String>,
    #[arg(long = "gids-", hide = true)]
    gids_remove: Vec<String>,

    /// List of Unicode codepoints. Use --unicodes+=... or --unicodes-=... to add to or remove
    /// from the set
    #[arg(short, long)]
    unicodes: Vec<String>,
    #[arg(long = "unicodes+", hide = true)]
    unicodes_add: Vec<String>,
    #[arg(long = "unicodes-", hide = true)]
    unicodes_remove: Vec<String>,

    /// Drop the specified tables. Use --drop-tables+=... or --drop-tables-=... to add to or
    /// remove from the default set
    #[arg(long)]
    drop_tables: Vec<String>,
    #[arg(long = "drop-tables+", hide = true)]
    drop_tables_add: Vec<String>,
    #[arg(long = "drop-tables-", hide = true)]
    drop_tables_remove: Vec<String>,

    /// List of layout features tags that will be preserved. Use --layout-features+=... or
    /// --layout-features-=... to add to or remove from the default set
    #[arg(long)]
    layout_features: Vec<String>,
    #[arg(long = "layout-features+", hide = true)]
    layout_features_add: Vec<String>,
    #[arg(long = "layout-features-", hide = true)]
    layout_features_remove: Vec<String>,

    /// List of layout script tags that will be preserved. Use --layout-scripts+=... or
    /// --layout-scripts-=... to add to or remove from the default set
    #[arg(long)]
    layout_scripts: Vec<String>,
    #[arg(long = "layout-scripts+", hide = true)]
    layout_scripts_add: Vec<String>,
    #[arg(long = "layout-scripts-", hide = true)]
    layout_scripts_remove: Vec<String>,

    /// List of 'name' table entry nameIDs. Use --name-IDs+=... or --name-IDs-=... to add to or
    /// remove from the default set
    #[arg(long = "name-IDs")]
    name_IDs: Vec<String>,
    #[arg(long = "name-IDs+", hide = true)]
    name_IDs_add: Vec<String>,
    #[arg(long = "name-IDs-", hide = true)]
    name_IDs_remove: Vec<String>,

    /// List of 'name' table entry langIDs. Use --name-languages+=... or --name-languages-=... to
    /// add to or remove from the default set
    #[arg(long)]
    name_languages: Vec<String>,
    #[arg(long = "name-languages+", hide = true)]
    name_languages_add: Vec<String>,
    #[arg(long = "name-languages-", hide = true)]
    name_languages_remove: Vec<String>,
}

/// How a list option changes its set.
#[derive(Clone, Copy)]
enum ListOp {
    Replace,
    Add,
    Remove,
}

/// Applies the values of list option `id` and its `+`/`-` variants (`<id>_add`, `<id>_remove`)
/// to `set`, in command line order. `parse` turns `*` into the set of all values.
///
/// Port of the list option callbacks in hb-subset:
/// <https://github.com/harfbuzz/harfbuzz/blob/a69b43c1f/util/hb-subset.cc#L200>
fn apply_list_args<T: Domain>(
    matches: &ArgMatches,
    id: &str,
    set: &mut IntSet<T>,
    parse: fn(&str) -> Result<IntSet<T>, SubsetError>,
) {
    let mut values = Vec::new();
    for (arg_id, op) in [
        (id.to_string(), ListOp::Replace),
        (format!("{id}_add"), ListOp::Add),
        (format!("{id}_remove"), ListOp::Remove),
    ] {
        if let (Some(indices), Some(args)) = (
            matches.indices_of(&arg_id),
            matches.get_many::<String>(&arg_id),
        ) {
            values.extend(indices.zip(args).map(|(index, arg)| (index, op, arg)));
        }
    }
    values.sort_unstable_by_key(|(index, _, _)| *index);

    for (_, op, arg) in values {
        let parsed = parse(arg).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        match op {
            ListOp::Replace => *set = parsed,
            ListOp::Add => set.union(&parsed),
            ListOp::Remove => set.subtract(&parsed),
        }
    }
}

fn main() {
    let matches = Args::command().get_matches();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let subset_flags = parse_subset_flags(&args);
    let mut gids = IntSet::<GlyphId>::empty();
    apply_list_args(&matches, "gids", &mut gids, populate_gids);

    let mut unicodes = IntSet::<u32>::empty();
    apply_list_args(&matches, "unicodes", &mut unicodes, parse_unicodes);

    let font_bytes = std::fs::read(&args.path)
        .unwrap_or_else(|err| panic!("Failed to read file {path:?}.\n{err}", path = &args.path));
    let font = FontRef::new(&font_bytes)
        .unwrap_or_else(|err| panic!("Failed to read {path:?} as font.\n{err}", path = &args.path));

    //default value: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-subset-input.cc#L46>
    let default_drop_tables = [
        // Layout disabled by default
        MORX,
        MORT,
        KERX,
        KERN,
        // Copied from fontTools
        JSTF,
        DSIG,
        ebdt::Ebdt::TAG,
        eblc::Eblc::TAG,
        EBSC,
        svg::Svg::TAG,
        PCLT,
        LTSH,
        // Graphite tables
        feat::Feat::TAG,
        GLAT,
        GLOC,
        SILF,
        SILL,
    ];
    let mut drop_tables: IntSet<Tag> = default_drop_tables.iter().copied().collect();
    apply_list_args(&matches, "drop_tables", &mut drop_tables, parse_tag_list);

    // default value: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-subset-input.cc#L43>
    let mut name_ids = IntSet::<NameId>::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    apply_list_args(&matches, "name_IDs", &mut name_ids, parse_name_ids);

    // default value: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-subset-input.cc#L44
    let mut name_languages = IntSet::<u16>::empty();
    name_languages.insert(0x0409);
    apply_list_args(
        &matches,
        "name_languages",
        &mut name_languages,
        parse_name_languages,
    );

    // default value: <https://github.com/harfbuzz/harfbuzz/blob/021b44388667903d7bc9c92c924ad079f13b90ce/src/hb-subset-input.cc#L189>
    let mut layout_scripts = IntSet::<Tag>::empty();
    layout_scripts.invert();
    apply_list_args(
        &matches,
        "layout_scripts",
        &mut layout_scripts,
        parse_tag_list,
    );

    // default value: <https://github.com/harfbuzz/harfbuzz/blob/689383d05b2609a67efa307a9860b85cf40bf8c3/src/hb-subset-input.cc#L82>
    let mut layout_features = IntSet::<Tag>::empty();
    layout_features.extend(DEFAULT_LAYOUT_FEATURES.iter().copied());
    apply_list_args(
        &matches,
        "layout_features",
        &mut layout_features,
        parse_tag_list,
    );

    let mut output_bytes = Vec::new();
    for _ in 0..args.num_iterations.unwrap_or(1) {
        let plan = Plan::new(
            &gids,
            &unicodes,
            &font,
            subset_flags,
            &drop_tables,
            &layout_scripts,
            &layout_features,
            &name_ids,
            &name_languages,
        );
        match subset_font(&font, &plan) {
            Ok(out) => {
                output_bytes = out;
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
    }
    std::fs::write(&args.output_file, output_bytes).unwrap_or_else(|err| {
        panic!(
            "Failed to write output to {path:?}.\n{err}",
            path = &args.output_file
        )
    });
}

fn parse_subset_flags(args: &Args) -> SubsetFlags {
    let mut flags = SubsetFlags::default();
    if args.no_hinting {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_HINTING;
    }

    if args.retain_gids {
        flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
    }

    if args.desubroutinize {
        flags |= SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE;
    }

    if args.name_legacy {
        flags |= SubsetFlags::SUBSET_FLAGS_NAME_LEGACY;
    }

    if args.set_overlaps_flag {
        flags |= SubsetFlags::SUBSET_FLAGS_SET_OVERLAPS_FLAG;
    }

    if args.notdef_outline {
        flags |= SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE;
    }

    if args.no_prune_unicode_ranges {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES;
    }

    if args.no_layout_closure {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_LAYOUT_CLOSURE;
    }

    if args.no_bidi_closure {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_BIDI_CLOSURE;
    }

    if args.glyph_names {
        flags |= SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES;
    }

    if args.passthrough_tables {
        flags |= SubsetFlags::SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED;
    }

    if args.retain_num_glyphs {
        flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS;
    }

    if args.optimize {
        flags |= SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS;
    }
    flags
}
