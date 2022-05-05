//! binary codegen tool.
//!
//! Takes a path to a template file as input, and writes the output to stdout

use std::path::{Path, PathBuf};

use font_codegen::ErrorReport;

fn main() -> miette::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| ErrorReport::message("missing path argument"))?;

    let generated_code = run_for_path(&path)?;
    println!("{generated_code}");

    Ok(())
}

fn run_for_path(path: &Path) -> miette::Result<String> {
    let contents = std::fs::read_to_string(&path).map_err(|_e| {
        ErrorReport::message(format!("error reading '{}': {}", path.display(), _e))
    })?;

    font_codegen::generate_code(&contents)
        .map_err(|e| ErrorReport::from_error_src(&e, &path, contents).into())
}
