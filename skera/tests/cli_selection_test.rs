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

fn cli_subset(input: &str, options: &[&str]) -> Vec<u8> {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("subset.ttf");
    let result = Command::new(env!("CARGO_BIN_EXE_skera"))
        .args(["--path", input, "--output-file"])
        .arg(&output)
        .args(options)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(output).unwrap()
}

#[test]
fn text_selectors_match_unicode_lists_and_follow_command_line_order() {
    for (input, options, expected) in [
        (
            "test-data/fonts/Roboto-Regular.ttf",
            vec!["--text=ABé"],
            "--unicodes=41,42,E9",
        ),
        (
            "test-data/fonts/Roboto-Regular.ttf",
            vec!["--unicodes=41,42", "--text=C"],
            "--unicodes=43",
        ),
        (
            "test-data/fonts/Roboto-Regular.ttf",
            vec!["--text=A", "--unicodes=42,43"],
            "--unicodes=42,43",
        ),
        (
            "test-data/fonts/Roboto-Regular.ttf",
            vec!["--text=A", "--text=B"],
            "--unicodes=42",
        ),
        (
            "test-data/fonts/varc-unrelated-axis.ttf",
            vec!["--text=각"],
            "--unicodes=AC01",
        ),
        (
            "test-data/fonts/Roboto-Regular.ttf",
            vec!["--text="],
            "--unicodes=",
        ),
    ] {
        assert_eq!(cli_subset(input, &options), cli_subset(input, &[expected]));
    }
}

#[test]
fn glyph_name_selectors_resolve_post_cff_and_string_fallbacks() {
    for (input, options, expected) in [
        (
            "test-data/fonts/AlegreyaSans-BlackItalic.ttf",
            vec!["--glyphs=.notdef,A,gid31,uni0043"],
            "--gids=0,3,31,32",
        ),
        (
            "test-data/fonts/AlegreyaSans-BlackItalic.ttf",
            vec!["--gids=3", "--glyphs=B,C"],
            "--gids=31,32",
        ),
        (
            "test-data/fonts/AlegreyaSans-BlackItalic.ttf",
            vec!["--glyphs=A,C", "--gids=31"],
            "--gids=31",
        ),
        (
            "test-data/fonts/cff1_seac.otf",
            vec!["--glyphs=Agrave"],
            "--gids=3",
        ),
        (
            "test-data/fonts/Roboto-Regular.abc.ttf",
            vec!["--glyphs="],
            "--gids=",
        ),
    ] {
        assert_eq!(cli_subset(input, &options), cli_subset(input, &[expected]));
    }
    let directory = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_skera"))
        .args([
            "--path",
            "test-data/fonts/Roboto-Regular.abc.ttf",
            "--glyphs=missing",
            "--output-file",
        ])
        .arg(directory.path().join("subset.ttf"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Invalid input glyph name missing"));
}
