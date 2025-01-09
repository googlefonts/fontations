//! IFT Graph
//!
//! This command inspects an IFT font an generates a representation of the extension graph formed by invalidating patches.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use clap::Parser;
use font_types::Tag;
use incremental_font_transfer::{
    font_patch::IncrementalFontPatchBase,
    patch_group::PatchInfo,
    patchmap::{intersecting_patches, PatchFormat, PatchUri, SubsetDefinition, UriTemplateError},
};
use read_fonts::{ReadError, TableProvider};
use skrifa::{FontRef, MetadataProvider};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "This command inspects an IFT font (https://w3c.github.io/IFT/Overview.html) an generates a representation of the extension graph formed by invalidating patches."
)]
struct Args {
    /// The input IFT font file.
    #[arg(short, long)]
    font: std::path::PathBuf,
}

fn standard_features() -> BTreeSet<Tag> {
    // Copied from harfbuzz:
    // https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-subset-input.cc#L82
    BTreeSet::from([
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
        //horizontal
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
        Tag::new(b"jalt"), // HarfBuzz doesn't use; others might
        //East Asian spacing
        Tag::new(b"chws"),
        Tag::new(b"vchw"),
        Tag::new(b"halt"),
        Tag::new(b"vhal"),
        //private
        Tag::new(b"Harf"),
        Tag::new(b"HARF"),
        Tag::new(b"Buzz"),
        Tag::new(b"BUZZ"),
        //shapers

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
    ])
}

fn get_feature_tags(font: &FontRef<'_>) -> Result<BTreeSet<Tag>, ReadError> {
    let standard_features = standard_features();

    let mut result: BTreeSet<Tag> = Default::default();

    if let Ok(gsub) = font.gsub() {
        for fr in gsub.feature_list()?.feature_records() {
            if standard_features.contains(&fr.feature_tag()) {
                continue;
            }

            result.insert(fr.feature_tag());
        }
    }

    if let Ok(gpos) = font.gpos() {
        for fr in gpos.feature_list()?.feature_records() {
            if standard_features.contains(&fr.feature_tag()) {
                continue;
            }

            result.insert(fr.feature_tag());
        }
    }

    Ok(result)
}

fn get_node_name(font: &FontRef<'_>) -> Result<String, ReadError> {
    let chars: BTreeSet<char> = font
        .charmap()
        .mappings()
        .map(|(cp, _)| char::from_u32(cp).unwrap())
        .collect();

    let mut name: String = chars.into_iter().collect();

    let features = get_feature_tags(font)?;
    if !features.is_empty() {
        let features: Vec<_> = features.into_iter().map(|t| t.to_string()).collect();
        let features = features.join(",");
        name.push('|');
        name.push_str(&features);
    }

    let axes = font.axes();
    if !axes.is_empty() {
        let axis_strings: Vec<_> = font
            .axes()
            .iter()
            .map(|axis| format!("{}[{}..{}]", axis.tag(), axis.min_value(), axis.max_value()))
            .collect();
        let axis_strings = axis_strings.join(",");
        name.push('|');
        name.push_str(&axis_strings);
    }

    Ok(name)
}

fn to_next_font(
    base_path: &Path,
    font: &FontRef<'_>,
    patch_uri: PatchUri,
) -> Result<Vec<u8>, UriTemplateError> {
    let path = base_path.join(patch_uri.uri_string()?);
    let patch_bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Unable to read patch file ({}): {:?}", path.display(), e));

    let patch_info: PatchInfo = patch_uri.try_into()?;

    Ok(font
        .apply_table_keyed_patch(&patch_info, &patch_bytes)
        .expect("Patch application failed."))
}

fn to_graph(
    base_path: &Path,
    font: FontRef<'_>,
    mut graph: BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeMap<String, BTreeSet<String>>, UriTemplateError> {
    let patches =
        intersecting_patches(&font, &SubsetDefinition::all()).expect("patch map parsing failed");

    let node_name = get_node_name(&font).unwrap();
    graph.entry(node_name.clone()).or_default();

    for patch in patches {
        if !matches!(patch.encoding(), PatchFormat::TableKeyed { .. }) {
            // This graph only considers invalidating (that is table keyed patches), so skip all other types.
            continue;
        }

        let next_font = to_next_font(base_path, &font, patch)?;
        let next_font = FontRef::new(&next_font).expect("Downstream font parsing failed");

        {
            let e = graph.entry(node_name.clone()).or_default();
            let next_node_name = get_node_name(&next_font).unwrap();
            e.insert(next_node_name);
        }

        graph = to_graph(base_path, next_font, graph)?
    }

    Ok(graph)
}

fn main() {
    let args = Args::parse();

    let font_bytes = std::fs::read(&args.font).unwrap_or_else(|e| {
        panic!(
            "Unable to read input font file ({}): {:?}",
            args.font.display(),
            e
        )
    });
    let font = FontRef::new(&font_bytes).expect("Input font parsing failed");
    let mut graph = Default::default();
    graph = to_graph(args.font.parent().unwrap(), font, graph)
        .unwrap_or_else(|_| panic!("Input font contains malformed URI templates."));

    for (key, values) in graph {
        let values: Vec<_> = values.into_iter().collect();
        println!("{key};{}", values.join(";"));
    }
}
