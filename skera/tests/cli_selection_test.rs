//! CLI selections are applied to the original font before instancing.
#![cfg(feature = "cli")]

use skera::{subset_font, Plan};
use std::process::Command;
use write_fonts::read::FontRef;

#[test]
fn collections_select_the_requested_face_and_default_to_the_first() {
    let input = "test-data/fonts/selection-collection.ttc";
    let source = std::fs::read(input).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("subset.ttf");
    let mut faces = Vec::new();
    for (option, index) in [
        (None, 0),
        (Some("--face-index=0"), 0),
        (Some("--face-index=1"), 1),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_skera"));
        command
            .args(["--path", input, "--keep-everything", "--output-file"])
            .arg(&output);
        if let Some(option) = option {
            command.arg(option);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = std::fs::read(&output).unwrap();
        let font = FontRef::from_index(&source, index).unwrap();
        assert_eq!(
            bytes,
            subset_font(&font, &Plan::keep_everything(&font)).unwrap()
        );
        faces.push(bytes);
    }
    assert_eq!(faces[0], faces[1]);
    assert_ne!(faces[0], faces[2]);
    let result = Command::new(env!("CARGO_BIN_EXE_skera"))
        .args([
            "--path",
            input,
            "--face-index=2",
            "--keep-everything",
            "--output-file",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
}
