//! build script for read-fonts.
//!
//! This copies certain test files into OUT_DIR if they exist, or writes
//! empty files if they do not exist.
//!
//! This means that the package can still compile even if these files are
//! missing, such as when packaged for crates.io

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../resources/test_fonts/");
    copy_files();
}

fn copy_files() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    for dir in [
        "../resources/test_fonts/ttf",
        "../resources/test_fonts/extracted",
    ] {
        for file in std::fs::read_dir(dir).unwrap() {
            let path = file.unwrap().path();
            let out_path = out_dir.join(path.file_name().unwrap());
            if path.exists() {
                std::fs::copy(path, out_path).unwrap();
            } else {
                std::fs::write(out_path, []).unwrap();
            }
        }
    }
}
