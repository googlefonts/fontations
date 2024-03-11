use std::io::Write;

use rayon::prelude::*;

use fauntlet::{compare_glyphs, InstanceOptions};
use skrifa::raw::types::F2Dot14;

#[derive(clap::Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Compare outlines and advances for all glyphs in a set of font files
    CompareGlyphs {
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

#[allow(clippy::explicit_write)]
fn main() {
    // Pixels per em sizes. A size of 0 means an explicit unscaled comparison
    let ppem_sizes = [0, 8, 16, 50, 72, 113, 144];

    // Locations in normalized variation space
    let var_locations = [-1.0, -0.32, 0.0, 0.42, 1.0].map(F2Dot14::from_f32);

    let hinting = Some(skrifa::outline::HintingMode::Smooth {
        lcd_subpixel: Some(skrifa::outline::LcdLayout::Horizontal),
        preserve_linear_metrics: false,
    });

    // let hinting = Some(skrifa::outline::HintingMode::Strong);

    // let hinting = None;

    use clap::Parser as _;
    let args = Args::parse_from(wild::args());

    match args.command {
        Command::CompareGlyphs {
            print_paths,
            exit_on_fail,
            files,
        } => {
            use std::sync::atomic::{AtomicBool, Ordering};
            let ok = AtomicBool::new(true);
            files.par_iter().for_each(|font_path| {
                if print_paths {
                    writeln!(std::io::stdout(), "[{font_path:?}]").unwrap();
                }
                if let Some(mut font_data) = fauntlet::Font::new(font_path) {
                    for font_ix in 0..font_data.count() {
                        for ppem in &ppem_sizes {
                            let axis_count = font_data.axis_count(font_ix) as usize;
                            if axis_count != 0 {
                                let mut coords = vec![];
                                for coord in &var_locations {
                                    coords.clear();
                                    coords.extend(std::iter::repeat(*coord).take(axis_count));
                                    let options = fauntlet::InstanceOptions::new(
                                        font_ix, *ppem, &coords, hinting,
                                    );
                                    if let Some(instances) = font_data.instantiate(&options) {
                                        if !compare_glyphs(
                                            font_path,
                                            &options,
                                            instances,
                                            exit_on_fail,
                                        ) {
                                            ok.store(false, Ordering::Release);
                                        }
                                    }
                                }
                            } else {
                                let options = InstanceOptions::new(font_ix, *ppem, &[], hinting);
                                if let Some(instances) = font_data.instantiate(&options) {
                                    if !compare_glyphs(font_path, &options, instances, exit_on_fail)
                                    {
                                        ok.store(false, Ordering::Release);
                                    }
                                }
                            }
                        }
                    }
                }
            });
            if !ok.load(Ordering::Acquire) {
                std::process::exit(1);
            }
        }
    }
}
