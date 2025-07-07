use std::io::Write;

use rayon::prelude::*;

use fauntlet::{compare_glyphs, Hinting, InstanceOptions};
use skrifa::raw::types::F2Dot14;

#[derive(clap::Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Specifies which hinting engine(s) to test.
#[derive(clap::ValueEnum, Copy, Clone, Default, Debug)]
enum HintingEngine {
    /// Disable hinting.
    #[default]
    None,
    /// The TrueType or CFF interpreter.
    Interpreter,
    /// The autohinter.
    Auto,
    /// Test with all hinting engines.
    All,
}

/// Specifies which hinting engine(s) to test.
#[derive(clap::ValueEnum, Copy, Clone, Default, Debug)]
enum HintingTarget {
    /// The standard hinting mode.
    #[default]
    Normal,
    /// The light hinting mode.
    Light,
    /// The horizontal LCD hinting mode.
    Lcd,
    /// The vertical LCD hinting mode.
    VerticalLcd,
    /// The monochrome hinting mode.
    Mono,
    /// Test with all hinting modes.
    All,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Compare outlines and advances for all glyphs in a set of font files
    CompareGlyphs {
        /// Print the path for each font file as it is processed
        #[arg(long)]
        print_paths: bool,
        /// Print the settings for each instance as it is processed
        #[arg(long)]
        print_instances: bool,
        /// The hinting engine to test
        #[arg(long)]
        hinting_engine: Option<HintingEngine>,
        /// The hinting target to test
        #[arg(long)]
        hinting_target: Option<HintingTarget>,
        #[arg(long)]
        /// End the process immediately if a comparison fails
        exit_on_fail: bool,
        /// Paths to font files to compare (may use glob syntax)
        files: Vec<std::path::PathBuf>,
    },
}

#[allow(clippy::explicit_write)]
fn main() {
    // Pixels per em sizes. A size of 0 means an explicit unscaled comparison.
    //
    // The 24.8 size exists to test rounding behavior for hinting. In particular,
    // we used to truncate, so choose a fractional part > 0.5 to ensure proper
    // rounding.
    // <https://github.com/googlefonts/fontations/issues/1544>
    let ppem_sizes = [0.0, 8.0, 16.0, 24.8, 50.0, 72.0, 113.0, 144.0];

    // Locations in normalized variation space
    let var_locations = [-1.0, -0.32, 0.0, 0.42, 1.0].map(F2Dot14::from_f32);

    use clap::Parser as _;
    let args = Args::parse_from(wild::args());

    match args.command {
        Command::CompareGlyphs {
            print_paths,
            print_instances,
            hinting_engine,
            hinting_target,
            exit_on_fail,
            files,
        } => {
            let hinting_matrix = hinting_modes(hinting_engine, hinting_target);
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
                                    coords.extend(std::iter::repeat_n(*coord, axis_count));
                                    for hinting in &hinting_matrix {
                                        let options = fauntlet::InstanceOptions::new(
                                            font_ix, *ppem, &coords, *hinting,
                                        );
                                        if print_instances {
                                            writeln!(
                                                std::io::stdout(),
                                                "<size: {}, coords: {:?}, hinting: {:?}>",
                                                *ppem,
                                                &coords,
                                                *hinting
                                            )
                                            .unwrap();
                                        }
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
                                }
                            } else {
                                for hinting in &hinting_matrix {
                                    let options =
                                        InstanceOptions::new(font_ix, *ppem, &[], *hinting);
                                    if print_instances {
                                        writeln!(
                                            std::io::stdout(),
                                            "<size: {}, hinting: {:?}>",
                                            *ppem,
                                            *hinting
                                        )
                                        .unwrap();
                                    }
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

fn hinting_modes(engine: Option<HintingEngine>, target: Option<HintingTarget>) -> Vec<Hinting> {
    let target = target.unwrap_or_default();
    let mut modes = vec![];
    match engine {
        None | Some(HintingEngine::None) => return vec![Hinting::None],
        Some(HintingEngine::Interpreter) => collect_hinting_modes(false, target, &mut modes),
        Some(HintingEngine::Auto) => collect_hinting_modes(true, target, &mut modes),
        Some(HintingEngine::All) => {
            collect_hinting_modes(false, target, &mut modes);
            collect_hinting_modes(true, target, &mut modes);
        }
    }
    modes
}

fn collect_hinting_modes(is_auto: bool, target: HintingTarget, modes: &mut Vec<Hinting>) {
    use fauntlet::HintingTarget::*;
    let actual_target = match target {
        HintingTarget::Normal => Normal,
        HintingTarget::Light => Light,
        HintingTarget::Lcd => Lcd,
        HintingTarget::VerticalLcd => VerticalLcd,
        HintingTarget::Mono => Mono,
        HintingTarget::All => {
            for target in [Normal, Light, Lcd, VerticalLcd, Mono] {
                if is_auto {
                    modes.push(Hinting::Auto(target))
                } else {
                    modes.push(Hinting::Interpreter(target))
                }
            }
            return;
        }
    };
    if is_auto {
        modes.push(Hinting::Auto(actual_target))
    } else {
        modes.push(Hinting::Interpreter(actual_target))
    }
}
