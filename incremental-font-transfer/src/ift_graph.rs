//! IFT Graph
//!
//! This command inspects an IFT font an generates a representation of the extension graph formed by invalidating patches.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use clap::Parser;

use incremental_font_transfer::{
    font_patch::IncrementalFontPatchBase,
    patch_group::PatchInfo,
    patchmap::{intersecting_patches, PatchFormat, PatchUri, SubsetDefinition},
};
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

fn get_node_name(font: &FontRef<'_>) -> String {
    // TODO(garretrieger): include features and design space
    let chars: BTreeSet<char> = font
        .charmap()
        .mappings()
        .map(|(cp, _)| char::from_u32(cp).unwrap())
        .collect();

    chars.into_iter().collect()
}

fn to_next_font(base_path: &Path, font: &FontRef<'_>, patch_uri: PatchUri) -> Vec<u8> {
    let path = base_path.join(patch_uri.uri_string());
    let patch_bytes = std::fs::read(path.clone())
        .unwrap_or_else(|e| panic!("Unable to read patch file ({}): {:?}", path.display(), e));

    let patch_info: PatchInfo = patch_uri.into();

    font.apply_table_keyed_patch(&patch_info, &patch_bytes)
        .expect("Patch application failed.")
}

fn to_graph(
    base_path: &Path,
    font: FontRef<'_>,
    mut graph: BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let patches =
        intersecting_patches(&font, &SubsetDefinition::all()).expect("patch map parsing failed");

    let node_name = get_node_name(&font);
    graph.entry(node_name.clone()).or_default();

    for patch in patches {
        match patch.encoding() {
            PatchFormat::TableKeyed { .. } => {}
            // This graph only considers invalidating (that is table keyed patches), so skip all other types.
            _ => continue,
        };

        let next_font = to_next_font(base_path, &font, patch);
        let next_font = FontRef::new(&next_font).expect("Downstream font parsing failed");

        {
            let e = graph.entry(node_name.clone()).or_default();
            let next_node_name = get_node_name(&next_font);
            e.insert(next_node_name);
        }

        graph = to_graph(base_path, next_font, graph)
    }

    graph
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
    graph = to_graph(&args.font.parent().unwrap(), font, graph);

    for (key, values) in graph {
        let values: Vec<_> = values.into_iter().collect();
        println!("{key};{}", values.join(","));
    }
}
