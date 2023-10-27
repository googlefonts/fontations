use std::path::Path;

use rayon::prelude::*;

use fauntlet::{FreeTypeInstance, InstanceOptions, RecordingPen, RegularizingPen, SkrifaInstance};
use skrifa::{raw::types::F2Dot14, GlyphId};

#[derive(clap::Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Compare outlines for all glyphs in a set of font files
    CompareOutlines {
        /// Print the path for each font file as it is processed
        #[arg(long)]
        print_paths: bool,
        #[arg(long)]
        /// End the process immediately if a comparison fails
        exit_on_fail: bool,
        /// Paths to font files to compare (may use glob syntax)
        files: Vec<std::path::PathBuf>,
    },
}

fn main() {
    // Pixels per em sizes
    let ppem_sizes = [0, 8, 16, 50, 72, 113, 144];

    // Locations in normalized variation space
    let var_locations = [-1.0, -0.32, 0.0, 0.42, 1.0].map(|c| F2Dot14::from_f32(c));

    use clap::Parser as _;
    let args = Args::parse_from(wild::args());

    match args.command {
        Command::CompareOutlines {
            print_paths,
            exit_on_fail,
            files,
        } => {
            files.par_iter().for_each(|font_path| {
                if print_paths {
                    println!("[{font_path:?}]");
                }
                if let Some(mut font_data) = fauntlet::Font::new(&font_path) {
                    for font_ix in 0..font_data.count() {
                        for ppem in &ppem_sizes {
                            let axis_count = font_data.axis_count(font_ix) as usize;
                            if axis_count != 0 {
                                let mut coords = vec![];
                                for coord in &var_locations {
                                    coords.clear();
                                    coords.extend(std::iter::repeat(*coord).take(axis_count));
                                    let options =
                                        fauntlet::InstanceOptions::new(font_ix, *ppem, &coords);
                                    if let Some(fonts) = font_data.instantiate(&options) {
                                        compare_outlines(&font_path, &options, fonts, exit_on_fail);
                                    }
                                }
                            } else {
                                let options = InstanceOptions::new(font_ix, *ppem, &[]);
                                if let Some(fonts) = font_data.instantiate(&options) {
                                    compare_outlines(&font_path, &options, fonts, exit_on_fail);
                                }
                            }
                        }
                    }
                }
            });
        }
    }
}

fn compare_outlines(
    path: &Path,
    options: &InstanceOptions,
    (mut ft_font, mut skrifa_font): (FreeTypeInstance, SkrifaInstance),
    exit_on_fail: bool,
) {
    let glyph_count = skrifa_font.glyph_count();
    let is_scaled = options.ppem != 0;

    let mut ft_outline = RecordingPen::default();
    let mut skrifa_outline = RecordingPen::default();

    for gid in 0..glyph_count {
        let gid = GlyphId::new(gid);
        ft_outline.clear();
        ft_font
            .outline(gid, &mut RegularizingPen::new(&mut ft_outline, is_scaled))
            .unwrap();
        skrifa_outline.clear();
        skrifa_font
            .outline(
                gid,
                &mut RegularizingPen::new(&mut skrifa_outline, is_scaled),
            )
            .unwrap();
        if ft_outline != skrifa_outline {
            fn outline_to_string(outline: &RecordingPen) -> String {
                outline
                    .0
                    .iter()
                    .map(|cmd| format!("{cmd:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            let ft_cmds = outline_to_string(&ft_outline);
            let skrifa_cmds = outline_to_string(&skrifa_outline);
            let diff = similar::TextDiff::from_lines(&ft_cmds, &skrifa_cmds);
            let mut diff_str = String::default();
            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    similar::ChangeTag::Delete => "-",
                    similar::ChangeTag::Insert => "+",
                    similar::ChangeTag::Equal => " ",
                };
                diff_str.push_str(&format!("{sign} {change}"));
            }
            println!(
                "[{path:?}#{} ppem: {} coords: {:?}] glyph id {} doesn't match:\n{diff_str}",
                options.index,
                options.ppem,
                options.coords,
                gid.to_u16(),
            );
            if exit_on_fail {
                std::process::exit(1);
            }
        }
    }
}
