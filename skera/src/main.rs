//! binary subset tool
//!
//! Takes a font file and a subset input which describes the desired subset, and output is a new
//! font file containing only the data specified in the input.
//!

use clap::Parser;
use skera::{
    parse_glyph_mapping, parse_name_ids, parse_name_languages, parse_tag_list, parse_unicodes,
    populate_gids, subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES, DSIG, EBSC, GLAT, GLOC,
    JSTF, KERN, KERX, LTSH, MORT, MORX, PCLT, SILF, SILL,
};
use write_fonts::read::{
    collections::IntSet,
    tables::{ebdt, eblc, feat, svg},
    types::{NameId, Tag},
    FontRef, TableProvider, TopLevelTable,
};

#[derive(Parser, Debug)]
//Allow name_IDs, so we keep the option name consistent with HB and fonttools
#[allow(non_snake_case)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input font file.
    #[arg(short, long)]
    path: std::path::PathBuf,

    /// List of glyph ids
    #[arg(short, long)]
    gids: Option<String>,

    /// Start with all glyphs, names, layout items, and tables selected.
    #[arg(long)]
    keep_everything: bool,

    /// Original:new glyph ID pairs preserving glyph order, for example 1:4,2:7.
    #[arg(long = "gid-map", alias = "glyph-map")]
    glyph_map: Option<String>,

    /// List of Unicode codepoints
    #[arg(short, long)]
    unicodes: Option<String>,

    /// The output font file
    #[arg(short, long)]
    output_file: std::path::PathBuf,

    /// Drop the specified tables.
    #[arg(long)]
    drop_tables: Option<String>,

    /// List of layout features tags that will be preserved
    #[arg(long)]
    layout_features: Option<String>,

    /// List of layout script tags that will be preserved
    #[arg(long)]
    layout_scripts: Option<String>,

    /// List of 'name' table entry nameIDs
    #[arg(long = "name-IDs", alias = "name-i-ds")]
    name_IDs: Option<String>,

    /// List of 'name' table entry langIDs
    #[arg(long)]
    name_languages: Option<String>,

    /// drop hints
    #[arg(long)]
    no_hinting: bool,

    /// If set don't renumber glyph ids in the subset.
    #[arg(long)]
    retain_gids: bool,

    /// With retained glyph IDs, keep the source glyph count using empty trailing glyphs.
    #[arg(long)]
    retain_num_glyphs: bool,

    /// Force 32-bit outline offsets for incremental font transfer patches.
    #[arg(long)]
    iftb_requirements: bool,

    /// Remove CFF/CFF2 use of subroutines
    #[arg(long)]
    desubroutinize: bool,

    /// Instance axes or ranges, for example wght=650,CNTR=drop or wght=300:500:700.
    #[arg(long, alias = "variations")]
    instance: Option<String>,

    /// Convert a full CFF2 instance to CFF1 outlines.
    #[arg(long)]
    downgrade_cff2: bool,

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

    /// Do not add Unicode codepoints for mirrored glyphs.
    #[arg(long)]
    no_bidi_closure: bool,

    /// Keep PS glyph names in TT-flavored fonts
    #[arg(long)]
    glyph_names: bool,

    /// Do not drop tables that the tool does not know how to subset
    #[arg(long)]
    passthrough_tables: bool,

    /// Perform IUP delta optimization on the resulting gvar table's deltas
    #[arg(long)]
    optimize: bool,

    /// Emit an identity CFF charset (CID = output GID) for CID-keyed CFF
    #[arg(long)]
    cff_identity_charset: bool,

    ///run subsetter N times
    #[arg(short, long)]
    num_iterations: Option<u32>,
}

fn main() {
    let args = Args::parse();

    let subset_flags = parse_subset_flags(&args);
    let glyph_mapping = parse_glyph_mapping(args.glyph_map.as_deref().unwrap_or_default())
        .unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        });
    let mut gids = match populate_gids(args.gids.as_deref().unwrap_or(if args.keep_everything {
        "*"
    } else {
        ""
    })) {
        Ok(gids) => gids,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    gids.extend(glyph_mapping.iter().map(|&(old, _)| old));

    let unicodes =
        match parse_unicodes(args.unicodes.as_deref().unwrap_or(if args.keep_everything {
            "*"
        } else {
            ""
        })) {
            Ok(unicodes) => unicodes,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };

    let font_bytes = std::fs::read(&args.path)
        .unwrap_or_else(|err| panic!("Failed to read file {path:?}.\n{err}", path = &args.path));
    let font = FontRef::new(&font_bytes)
        .unwrap_or_else(|err| panic!("Failed to read {path:?} as font.\n{err}", path = &args.path));
    let drop_tables = match &args.drop_tables {
        Some(drop_tables_input) => match parse_tag_list(drop_tables_input) {
            Ok(drop_tables) => drop_tables,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        //default value: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-subset-input.cc#L46>
        None if args.keep_everything => IntSet::empty(),
        None => {
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
            let drop_tables: IntSet<Tag> = default_drop_tables.iter().copied().collect();
            drop_tables
        }
    };

    let name_ids = match &args.name_IDs {
        Some(name_ids_input) => match parse_name_ids(name_ids_input) {
            Ok(name_ids) => name_ids,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        // default value: <https://github.com/harfbuzz/harfbuzz/blob/b5a65e0f20c30a7f13b2f6619479a6d666e603e0/src/hb-subset-input.cc#L43>
        None if args.keep_everything => IntSet::all(),
        None => {
            let mut default_name_ids = IntSet::<NameId>::empty();
            default_name_ids.insert_range(NameId::from(0)..=NameId::from(6));
            default_name_ids
        }
    };

    let name_languages = match &args.name_languages {
        Some(name_languages_input) => match parse_name_languages(name_languages_input) {
            Ok(name_languages) => name_languages,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        // default value: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-subset-input.cc#L44
        None if args.keep_everything => IntSet::all(),
        None => {
            let mut default_name_languages = IntSet::<u16>::empty();
            default_name_languages.insert(0x0409);
            default_name_languages
        }
    };

    let layout_scripts = match &args.layout_scripts {
        Some(layout_scripts_input) => match parse_tag_list(layout_scripts_input) {
            Ok(layout_scripts) => layout_scripts,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        // default value: <https://github.com/harfbuzz/harfbuzz/blob/021b44388667903d7bc9c92c924ad079f13b90ce/src/hb-subset-input.cc#L189>
        None => {
            let mut default_layout_scripts = IntSet::<Tag>::empty();
            default_layout_scripts.invert();
            default_layout_scripts
        }
    };

    let layout_features = match &args.layout_features {
        Some(layout_features_input) => match parse_tag_list(layout_features_input) {
            Ok(layout_features) => layout_features,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        // default value: <https://github.com/harfbuzz/harfbuzz/blob/689383d05b2609a67efa307a9860b85cf40bf8c3/src/hb-subset-input.cc#L82>
        None if args.keep_everything => IntSet::all(),
        None => {
            let mut default_layout_features = IntSet::<Tag>::empty();
            default_layout_features.extend(DEFAULT_LAYOUT_FEATURES.iter().copied());
            default_layout_features
        }
    };

    // Instancing must consider only retained VARC references. Preserve source
    // glyph IDs and count during this preliminary subset so all selectors and
    // custom mappings still refer to the original glyph IDs in the final plan.
    let varc_subset = (args.instance.is_some() && font.data_for_tag(Tag::new(b"VARC")).is_some())
        .then(|| {
            let plan = Plan::new(
                &gids,
                &unicodes,
                &font,
                subset_flags
                    | SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
                    | SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS,
                &drop_tables,
                &layout_scripts,
                &layout_features,
                &name_ids,
                &name_languages,
            );
            subset_font(&font, &plan).unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            })
        });
    let font = varc_subset
        .as_deref()
        .map(FontRef::new)
        .transpose()
        .unwrap()
        .unwrap_or(font);
    let instance_bytes = args.instance.as_deref().map(|input| {
        skera::parse_axis_limits(input)
            .and_then(|limits| skera::instance_font_with_flags(&font, &limits, subset_flags))
            .unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            })
    });
    let font = instance_bytes
        .as_deref()
        .map(FontRef::new)
        .transpose()
        .unwrap()
        .unwrap_or(font);
    let cff1_bytes =
        (args.downgrade_cff2 && font.cff2().is_ok() && font.fvar().is_err()).then(|| {
            skera::downgrade_cff2(&font).unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            })
        });
    let font = cff1_bytes
        .as_deref()
        .map(FontRef::new)
        .transpose()
        .unwrap()
        .unwrap_or(font);

    let mut output_bytes = Vec::new();
    for _ in 0..args.num_iterations.unwrap_or(1) {
        let mut plan = Plan::new(
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
        if args.glyph_map.is_some() {
            plan.set_glyph_mapping(&glyph_mapping)
                .unwrap_or_else(|err| {
                    eprintln!("{err}");
                    std::process::exit(1);
                });
        }
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
    let mut flags = if args.keep_everything {
        SubsetFlags::KEEP_EVERYTHING
    } else {
        SubsetFlags::default()
    };
    if args.no_hinting {
        flags |= SubsetFlags::SUBSET_FLAGS_NO_HINTING;
    }

    if args.retain_gids {
        flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
    }

    if args.retain_num_glyphs {
        flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_NUM_GLYPHS;
    }

    if args.iftb_requirements {
        flags |= SubsetFlags::SUBSET_FLAGS_IFTB_REQUIREMENTS;
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

    if args.optimize {
        flags |= SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS;
    }
    if args.cff_identity_charset {
        flags |= SubsetFlags::SUBSET_FLAGS_CFF_IDENTITY_CHARSET;
    }
    if args.downgrade_cff2 {
        flags |= SubsetFlags::SUBSET_FLAGS_DOWNGRADE_CFF2;
    }
    flags
}
