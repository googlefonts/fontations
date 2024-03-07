//! Generate consts for colrv1_json testdata

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

fn main() {
    // Scan the whole json dir, works after https://github.com/rust-lang/cargo/pull/8973
    println!("cargo:rerun-if-changed=font-test-data/test_data/colrv1_json");

    let out_dir = PathBuf::from_str(&std::env::var("OUT_DIR").unwrap()).unwrap();
    let out_file = out_dir.join("colrv1_json.rs");

    // Cargo ensures working directory is set so relative paths should be safe
    // <https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-build-scripts>
    let json_dir = Path::new("test_data/colrv1_json");
    assert!(json_dir.is_dir(), "{json_dir:?} should be a directory");

    let mut content = String::new();
    content
        .push_str("pub fn content(set_name: &str, settings: &[(&str, f32)]) -> &'static str {\n");
    content.push_str("    let mut key = Vec::with_capacity(1 + settings.len());\n");
    content.push_str("    key.push(set_name.to_ascii_lowercase());\n");
    content.push_str("    key.extend(settings.iter().map(|(t, v)| format!(\"{t}_{v}\")));\n");
    content.push_str("    let key = key.join(\"_\");\n");
    content.push_str("    match key.as_str() {\n");

    for dir_entry in fs::read_dir(json_dir).unwrap() {
        let path = dir_entry.unwrap().path();
        let basename = path.file_name().unwrap().to_str().unwrap();

        // finding things under src from OUT_DIR is weird, just copy the file
        fs::copy(&path, out_dir.join(basename)).unwrap();

        content.push_str(&format!(
            "       \"{basename}\" => include_str!(\"{basename}\"),\n"
        ));
    }
    content.push_str("       _ => panic!(\"No data available for {key}\"),\n");
    content.push_str("    }\n");
    content.push_str("}\n");

    fs::write(out_file, content).unwrap();
}
