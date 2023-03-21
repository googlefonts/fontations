//! build script for read-fonts.
//!
//! This copies certain test files into OUT_DIR if they exist, or writes
//! empty files if they do not exist.
//!
//! This means that the package can still compile even if these files are
//! missing, such as when packaged for crates.io

use std::path::Path;

static FILES_TO_MOVE: &[&str] = &[
    "../resources/test_fonts/ttf/cmap14_font1.ttf",
    "../resources/test_fonts/ttf/linear_gradient_rect_colr_1.ttf",
    "../resources/test_fonts/ttf/simple_glyf.ttf",
    "../resources/test_fonts/ttf/vazirmatn_var_trimmed.ttf",
    "../resources/test_fonts/extracted/vazirmatn_var_trimmed-glyphs.txt",
];

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    for path in FILES_TO_MOVE.iter().map(Path::new) {
        let out_path = out_dir.join(path.file_name().unwrap());
        if path.exists() {
            std::fs::copy(path, out_path).unwrap();
        } else {
            std::fs::write(out_path, []).unwrap();
        }
    }
}
