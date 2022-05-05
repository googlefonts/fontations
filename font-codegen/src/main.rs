//! binary codegen tool.
//!
//! Takes a path to a template file as input, and writes the output to stdout

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use font_codegen::ErrorReport;
use miette::miette;
use serde::Deserialize;

fn main() -> miette::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| ErrorReport::message("missing path argument"))?;

    if path.extension() == Some(OsStr::new("toml")) {
        return run_plan(&path);
    }

    let generated_code = run_for_path(&path)?;
    println!("{generated_code}");

    Ok(())
}

fn run_plan(path: &Path) -> miette::Result<()> {
    ensure_correct_working_directory()?;
    let contents = read_contents(path)?;
    let plan: CodegenPlan =
        toml::from_str(&contents).map_err(|e| miette!("failed to parse plan: '{}'", e))?;

    for path in &plan.clean {
        if path.exists() {
            println!("removing {}", path.display());
            std::fs::remove_dir_all(path)
                .map_err(|e| miette!("failed to clean directory '{}': {e}", path.display()))?;
        }
        println!("creating {}", path.display());
        std::fs::create_dir_all(path)
            .map_err(|e| miette!("failed to create directory '{}': {e}", path.display()))?;
    }
    let results = plan
        .generate
        .iter()
        .map(|op| run_for_path(&op.source))
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
    source: PathBuf,
    target: PathBuf,
}

fn read_contents(path: &Path) -> miette::Result<String> {
    std::fs::read_to_string(&path).map_err(|e| {
        { ErrorReport::message(format!("error reading '{}': {}", path.display(), e)) }.into()
    })
}

fn run_for_path(path: &Path) -> miette::Result<String> {
    let contents = read_contents(path)?;
    font_codegen::generate_code(&contents)
        .map_err(|e| ErrorReport::from_error_src(&e, path, contents).into())
}
