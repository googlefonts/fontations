//! binary codegen tool.
//!
//! Takes a path to a template file as input, and writes the output to stdout

use std::path::{Path, PathBuf};

use font_codegen::{ErrorReport, Mode};
use miette::miette;
use serde::Deserialize;

fn main() -> miette::Result<()> {
    match flags::Args::from_env() {
        Ok(args) => match args.subcommand {
            flags::ArgsCmd::Plan(plan) => run_plan(&plan.path),
            flags::ArgsCmd::File(args) => {
                let generated_code = run_for_path(&args.path, args.mode)?;
                print!("{generated_code}");
                Ok(())
            }
        },
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn run_plan(path: &Path) -> miette::Result<()> {
    ensure_correct_working_directory()?;
    let contents = read_contents(path)?;
    let plan: CodegenPlan =
        toml::from_str(&contents).map_err(|e| miette!("failed to parse plan: '{}'", e))?;

    for path in &plan.clean {
        if path.exists() {
            println!("removing {}", path.display());
            if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(&path)
            }
            .map_err(|e| miette!("failed to clean path '{}': {e}", path.display()))?;
        }

        if path.is_dir() {
            println!("creating {}", path.display());
            std::fs::create_dir_all(path)
                .map_err(|e| miette!("failed to create directory '{}': {e}", path.display()))?;
        }
    }
    let results = plan
        .generate
        .iter()
        .map(|op| run_for_path(&op.source, op.mode))
        .collect::<Result<Vec<_>, _>>()?;

    for (op, generated) in plan.generate.iter().zip(results.iter()) {
        println!(
            "writing {} bytes to {}",
            generated.len(),
            op.target.display()
        );
        std::fs::write(&op.target, generated)
            .map_err(|e| miette!("error writing '{}': {}", op.target.display(), e))?;
    }
    Ok(())
}

fn ensure_correct_working_directory() -> miette::Result<()> {
    if !(Path::new("font-tables").is_dir() && Path::new("resources").is_dir()) {
        return Err(miette!(
            "codegen tool must be run from the root of the workspace"
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct CodegenPlan {
    generate: Vec<CodegenOp>,
    clean: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct CodegenOp {
    mode: Mode,
    source: PathBuf,
    target: PathBuf,
}

fn read_contents(path: &Path) -> miette::Result<String> {
    std::fs::read_to_string(&path).map_err(|e| {
        { ErrorReport::message(format!("error reading '{}': {}", path.display(), e)) }.into()
    })
}

fn run_for_path(path: &Path, mode: Mode) -> miette::Result<String> {
    let contents = read_contents(path)?;
    font_codegen::generate_code(&contents, mode)
        .map_err(|e| ErrorReport::from_error_src(&e, path, contents).into())
}

mod flags {
    use font_codegen::Mode;
    use std::path::PathBuf;

    xflags::xflags! {
        /// Generate font table representations
        cmd args {
            cmd file
                /// Code to generate; either 'parse' or 'compile'.
                required mode: Mode
                /// Path to the input file
                required path: PathBuf
                {}
            default cmd plan
                /// plan path
                required path: PathBuf {}
        }
    }
}
