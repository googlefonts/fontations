//! IFT Extension
//!
//! This command line tool executes the IFT extension algorithm (<https://w3c.github.io/IFT/Overview.html#extend-font-subset>) on an IFT font.

use std::collections::HashMap;

use clap::Parser;
use incremental_font_transfer::{
    patch_group::{PatchGroup, UriStatus},
    patchmap::SubsetDefinition,
};
use read_fonts::collections::IntSet;
use skrifa::FontRef;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Run the IFT extension algorithm (https://w3c.github.io/IFT/Overview.html#extend-font-subset) on an IFT font."
)]
struct Args {
    /// The input IFT font file.
    #[arg(short, long)]
    font: std::path::PathBuf,

    /// The output IFT font file.
    #[arg(short, long)]
    output: std::path::PathBuf,

    /// Text to extend the font to cover.
    #[arg(short, long)]
    text: Option<String>,

    /// Comma separate list of unicode codepoint values (base 10).
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    unicodes: Vec<String>,
    // TODO(garretrieger): add design space and feature tags arguments.
}

fn main() {
    let args = Args::parse();

    let mut codepoints = IntSet::<u32>::empty();
    if let Some(text) = args.text {
        codepoints.extend_unsorted(text.chars().map(|c| c as u32));
    }

    for unicode_string in args.unicodes {
        let unicode: u32 = unicode_string.parse().expect("bad unicode value");
        codepoints.insert(unicode);
    }

    let subset_definition = SubsetDefinition::codepoints(codepoints);

    let mut font_bytes = std::fs::read(&args.font).unwrap_or_else(|e| {
        panic!(
            "Unable to read input font file ({}): {:?}",
            args.font.display(),
            e
        )
    });

    let mut patch_data: HashMap<String, UriStatus> = Default::default();
    let mut it_count = 0;
    loop {
        it_count += 1;
        println!(">> Iteration {}", it_count);
        let font = FontRef::new(&font_bytes).expect("Input font parsing failed");
        let next_patches = PatchGroup::select_next_patches(font, &subset_definition)
            .expect("Patch selection failed");
        if !next_patches.has_uris() {
            println!("  No outstanding patches, all done.");
            break;
        }

        println!("  Selected URIs:");
        for uri in next_patches.uris() {
            println!("    fetching {}", uri);
            let uri_path = args.font.parent().unwrap().join(uri);
            let patch_bytes = std::fs::read(uri_path.clone()).unwrap_or_else(|e| {
                panic!(
                    "Unable to read patch file ({}): {:?}",
                    uri_path.display(),
                    e
                )
            });

            patch_data.insert(uri.to_string(), UriStatus::Pending(patch_bytes));
        }

        println!("  Applying patches");
        font_bytes = next_patches
            .apply_next_patches(&mut patch_data)
            .expect("Patch application failed.");
    }

    println!(">> Extension finished");
    std::fs::write(&args.output, font_bytes).expect("Writing output font failed.");
    println!(">> Wrote patched font to {}", &args.output.display());
}
