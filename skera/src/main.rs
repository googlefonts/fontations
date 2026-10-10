//! binary subset tool
//!
//! Takes a font file and a subset input which describes the desired subset, and output is a new
//! font file containing only the data specified in the input.
//!

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use skera::{
    parse_glyph_mapping, parse_glyph_names, parse_name_ids, parse_name_languages, parse_tag_list,
    parse_unicodes, populate_gids, subset_font, Plan, SubsetError, SubsetFlags,
    DEFAULT_LAYOUT_FEATURES, DSIG, EBSC, GLAT, GLOC, JSTF, KERN, KERX, LTSH, MORT, MORX, PCLT,
    SILF, SILL,
};
use write_fonts::read::{
    collections::{int_set::Domain, IntSet},
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

    /// Face index in a TrueType or OpenType collection.
    #[arg(short = 'y', long, default_value_t = 0)]
    face_index: u32,

    /// List of glyph ids
    #[arg(short, long, action = clap::ArgAction::Append)]
    gids: Vec<String>,

    /// Add glyph IDs or ranges to the current selection.
    #[arg(long = "gids+", action = clap::ArgAction::Append)]
    gids_add: Vec<String>,

    /// Remove glyph IDs or ranges from the current selection.
    #[arg(long = "gids-", action = clap::ArgAction::Append)]
    gids_remove: Vec<String>,

    /// Read gids selections from a file, or - for standard input.
    #[arg(long, action = clap::ArgAction::Append)]
    gids_file: Vec<String>,

    /// Glyph names or glyph strings (for example A,gid42,uni0041).
    #[arg(long, action = clap::ArgAction::Append)]
    glyphs: Vec<String>,

    /// Add glyph names to the current selection.
    #[arg(long = "glyphs+", action = clap::ArgAction::Append)]
    glyphs_add: Vec<String>,

    /// Remove glyph names from the current selection.
    #[arg(long = "glyphs-", action = clap::ArgAction::Append)]
    glyphs_remove: Vec<String>,

    /// Read glyphs selections from a file, or - for standard input.
    #[arg(long, action = clap::ArgAction::Append)]
    glyphs_file: Vec<String>,

    /// Start with all glyphs, names, layout items, and tables selected.
    #[arg(long)]
    keep_everything: bool,

    /// Original:new glyph ID pairs preserving glyph order, for example 1:4,2:7.
    #[arg(long = "gid-map", alias = "glyph-map")]
    glyph_map: Option<String>,

    /// List of Unicode codepoints
    #[arg(short, long, action = clap::ArgAction::Append)]
    unicodes: Vec<String>,

    /// Add Unicode codepoints or ranges to the current selection.
    #[arg(long = "unicodes+", action = clap::ArgAction::Append)]
    unicodes_add: Vec<String>,

    /// Remove Unicode codepoints or ranges from the current selection.
    #[arg(long = "unicodes-", action = clap::ArgAction::Append)]
    unicodes_remove: Vec<String>,

    /// Read unicodes selections from a file, or - for standard input.
    #[arg(long, action = clap::ArgAction::Append)]
    unicodes_file: Vec<String>,

    /// Text whose Unicode characters will be included in the subset.
    #[arg(short = 't', long, action = clap::ArgAction::Append)]
    text: Vec<String>,

    /// Add text's Unicode characters to the current selection.
    #[arg(long = "text+", action = clap::ArgAction::Append)]
    text_add: Vec<String>,

    /// Remove text's Unicode characters from the current selection.
    #[arg(long = "text-", action = clap::ArgAction::Append)]
    text_remove: Vec<String>,

    /// Read text selections from a file, or - for standard input.
    #[arg(long, action = clap::ArgAction::Append)]
    text_file: Vec<String>,

    /// The output font file
    #[arg(short, long)]
    output_file: std::path::PathBuf,

    /// Drop the specified tables.
    #[arg(long, action = clap::ArgAction::Append)]
    drop_tables: Vec<String>,

    /// Add tables to the current drop set.
    #[arg(long = "drop-tables+", action = clap::ArgAction::Append)]
    drop_tables_add: Vec<String>,

    /// Remove tables from the current drop set.
    #[arg(long = "drop-tables-", action = clap::ArgAction::Append)]
    drop_tables_remove: Vec<String>,

    /// List of layout features tags that will be preserved
    #[arg(long, action = clap::ArgAction::Append)]
    layout_features: Vec<String>,

    /// Add layout feature tags to the current selection.
    #[arg(long = "layout-features+", action = clap::ArgAction::Append)]
    layout_features_add: Vec<String>,

    /// Remove layout feature tags from the current selection.
    #[arg(long = "layout-features-", action = clap::ArgAction::Append)]
    layout_features_remove: Vec<String>,

    /// List of layout script tags that will be preserved
    #[arg(long, action = clap::ArgAction::Append)]
    layout_scripts: Vec<String>,

    /// Add layout script tags to the current selection.
    #[arg(long = "layout-scripts+", action = clap::ArgAction::Append)]
    layout_scripts_add: Vec<String>,

    /// Remove layout script tags from the current selection.
    #[arg(long = "layout-scripts-", action = clap::ArgAction::Append)]
    layout_scripts_remove: Vec<String>,

    /// List of 'name' table entry nameIDs
    #[arg(long = "name-IDs", alias = "name-i-ds", action = clap::ArgAction::Append)]
    name_IDs: Vec<String>,

    /// Add name IDs to the current selection.
    #[arg(long = "name-IDs+", action = clap::ArgAction::Append)]
    name_IDs_add: Vec<String>,

    /// Remove name IDs from the current selection.
    #[arg(long = "name-IDs-", action = clap::ArgAction::Append)]
    name_IDs_remove: Vec<String>,

    /// List of 'name' table entry langIDs
    #[arg(long, action = clap::ArgAction::Append)]
    name_languages: Vec<String>,

    /// Add name language IDs to the current selection.
    #[arg(long = "name-languages+", action = clap::ArgAction::Append)]
    name_languages_add: Vec<String>,

    /// Remove name language IDs from the current selection.
    #[arg(long = "name-languages-", action = clap::ArgAction::Append)]
    name_languages_remove: Vec<String>,

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
    let matches = Args::command().get_matches();
    let args = Args::from_arg_matches(&matches).unwrap();

    let subset_flags = parse_subset_flags(&args);
    let glyph_mapping = parse_glyph_mapping(args.glyph_map.as_deref().unwrap_or_default())
        .unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        });
    let mut unicodes = if args.keep_everything {
        IntSet::all()
    } else {
        IntSet::empty()
    };
    // Like HarfBuzz's parse_text/parse_unicodes, each plain selector replaces
    // the current Unicode set. Apply mixed selectors in command-line order.
    for (_, name, input) in selector_values(
        &matches,
        &[
            "unicodes",
            "unicodes_add",
            "unicodes_remove",
            "unicodes_file",
            "text",
            "text_add",
            "text_remove",
            "text_file",
        ],
    ) {
        let from_file = name.ends_with("_file");
        let inputs = if from_file {
            read_selector_file(input, !name.starts_with("text"))
        } else {
            vec![input.to_owned()]
        };
        for input in inputs {
            let selected = if name.starts_with("text") {
                if input == "*" {
                    IntSet::all()
                } else {
                    input.chars().map(u32::from).collect()
                }
            } else {
                parse_unicodes(&input).unwrap_or_else(|err| {
                    eprintln!("{err}");
                    std::process::exit(1);
                })
            };
            apply_selection(&mut unicodes, name, selected);
        }
    }

    let font_bytes = std::fs::read(&args.path)
        .unwrap_or_else(|err| panic!("Failed to read file {path:?}.\n{err}", path = &args.path));
    let font = FontRef::from_index(&font_bytes, args.face_index)
        .unwrap_or_else(|err| panic!("Failed to read {path:?} as font.\n{err}", path = &args.path));
    let mut gids = if args.keep_everything {
        IntSet::all()
    } else {
        IntSet::empty()
    };
    for (_, name, input) in selector_values(
        &matches,
        &[
            "gids",
            "gids_add",
            "gids_remove",
            "gids_file",
            "glyphs",
            "glyphs_add",
            "glyphs_remove",
            "glyphs_file",
        ],
    ) {
        let from_file = name.ends_with("_file");
        let inputs = if from_file {
            read_selector_file(input, true)
        } else {
            vec![input.to_owned()]
        };
        for input in inputs {
            let selected = if name.starts_with("glyphs") {
                parse_glyph_names(&font, &input)
            } else {
                populate_gids(&input)
            }
            .unwrap_or_else(|err| {
                eprintln!("{err}");
                std::process::exit(1);
            });
            apply_selection(&mut gids, name, selected);
        }
    }
    gids.extend(glyph_mapping.iter().map(|&(old, _)| old));
    let default_drop_tables = if args.keep_everything {
        IntSet::empty()
    } else {
        [
            // Layout disabled by default.
            MORX,
            MORT,
            KERX,
            KERN,
            // Copied from fontTools.
            JSTF,
            DSIG,
            ebdt::Ebdt::TAG,
            eblc::Eblc::TAG,
            EBSC,
            svg::Svg::TAG,
            PCLT,
            LTSH,
            // Graphite tables.
            feat::Feat::TAG,
            GLAT,
            GLOC,
            SILF,
            SILL,
        ]
        .into_iter()
        .collect()
    };
    let drop_tables = select_options(
        &matches,
        &["drop_tables", "drop_tables_add", "drop_tables_remove"],
        default_drop_tables,
        parse_tag_list,
    );
    let default_name_ids = if args.keep_everything {
        IntSet::all()
    } else {
        let mut ids = IntSet::empty();
        ids.insert_range(NameId::new(0)..=NameId::new(6));
        ids
    };
    let name_ids = select_options(
        &matches,
        &["name_IDs", "name_IDs_add", "name_IDs_remove"],
        default_name_ids,
        parse_name_ids,
    );
    let name_languages = select_options(
        &matches,
        &[
            "name_languages",
            "name_languages_add",
            "name_languages_remove",
        ],
        if args.keep_everything {
            IntSet::all()
        } else {
            [0x0409].into_iter().collect()
        },
        parse_name_languages,
    );
    let layout_scripts = select_options(
        &matches,
        &[
            "layout_scripts",
            "layout_scripts_add",
            "layout_scripts_remove",
        ],
        IntSet::all(),
        parse_tag_list,
    );
    let layout_features = select_options(
        &matches,
        &[
            "layout_features",
            "layout_features_add",
            "layout_features_remove",
        ],
        if args.keep_everything {
            IntSet::all()
        } else {
            DEFAULT_LAYOUT_FEATURES.iter().copied().collect()
        },
        parse_tag_list,
    );

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

// Clap retains the positions of every option value, including repeated
// selectors. HarfBuzz's callbacks naturally process them in the same order.
fn selector_values<'a>(
    matches: &'a ArgMatches,
    names: &[&'static str],
) -> Vec<(usize, &'static str, &'a str)> {
    let mut values = Vec::new();
    for &name in names {
        if let (Some(indices), Some(inputs)) =
            (matches.indices_of(name), matches.get_many::<String>(name))
        {
            values.extend(
                indices
                    .zip(inputs)
                    .map(|(i, input)| (i, name, input.as_str())),
            );
        }
    }
    values.sort_unstable_by_key(|v| v.0);
    values
}

// Follow HarfBuzz's callbacks: plain selectors replace, + and file selectors
// add, and - selectors remove. IntSet supports these operations on inverted
// sets without enumerating the domain.
fn apply_selection<T: Domain>(set: &mut IntSet<T>, name: &str, selected: IntSet<T>) {
    if name.ends_with("_remove") {
        set.subtract(&selected);
    } else if name.ends_with("_add") || name.ends_with("_file") {
        set.union(&selected);
    } else {
        *set = selected;
    }
}

fn select_options<T: Domain>(
    matches: &ArgMatches,
    names: &[&'static str],
    mut selected: IntSet<T>,
    parse: impl Fn(&str) -> Result<IntSet<T>, SubsetError>,
) -> IntSet<T> {
    for (_, name, input) in selector_values(matches, names) {
        let input = parse(input).unwrap_or_else(|err| {
            eprintln!("{err}");
            std::process::exit(1);
        });
        apply_selection(&mut selected, name, input);
    }
    selected
}

// HarfBuzz's file callbacks append each line. Numeric/name files allow #
// comments; text files retain # and omit only their line-feed separators.
fn read_selector_file(path: &str, comments: bool) -> Vec<String> {
    use std::io::Read;
    let input = if path == "-" {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).map(|_| input)
    } else {
        std::fs::read_to_string(path)
    }
    .unwrap_or_else(|err| {
        eprintln!("Failed reading selector file {path:?}: {err}");
        std::process::exit(1);
    });
    input
        .split('\n')
        .map(|line| {
            if comments {
                line.split('#').next().unwrap_or("").to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect()
}
