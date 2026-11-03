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

#[test]
fn selection_files_add_lines_and_keep_text_hash_characters() {
    let directory = tempfile::tempdir().unwrap();
    for (kind, input, contents, expected) in [
        (
            "gids",
            "test-data/fonts/Roboto-Regular.abc.ttf",
            "1 2\n# comment\n3 # tail\n",
            "--gids=1,2,3",
        ),
        (
            "unicodes",
            "test-data/fonts/Roboto-Regular.abc.ttf",
            "U+0061,0062\n# comment\n0063 # tail\n",
            "--unicodes=61,62,63",
        ),
        (
            "glyphs",
            "test-data/fonts/AlegreyaSans-BlackItalic.ttf",
            "A B\n# comment\nC\n",
            "--gids=3,31,32",
        ),
        (
            "text",
            "test-data/fonts/Roboto-Regular.abc.ttf",
            "a#b\nc\n",
            "--unicodes=61,23,62,63",
        ),
    ] {
        let file = directory.path().join(format!("{kind}.txt"));
        std::fs::write(&file, contents).unwrap();
        let option = format!("--{kind}-file={}", file.display());
        assert_eq!(
            cli_subset(input, &[&option]),
            cli_subset(input, &[expected])
        );
    }
    let file = directory.path().join("more.txt");
    std::fs::write(&file, "b\n").unwrap();
    let option = format!("--text-file={}", file.display());
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    assert_eq!(
        cli_subset(input, &["--text=a", &option]),
        cli_subset(input, &["--text=ab"])
    );
    assert_eq!(
        cli_subset(input, &[&option, "--text=c"]),
        cli_subset(input, &["--text=c"])
    );
}

#[test]
fn selector_files_can_read_standard_input_and_report_missing_files() {
    use std::io::Write;
    use std::process::Stdio;
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("subset.ttf");
    let mut child = Command::new(env!("CARGO_BIN_EXE_skera"))
        .args(["--path", input, "--gids-file=-", "--output-file"])
        .arg(&output)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"1 2\n3 # comment\n")
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        std::fs::read(&output).unwrap(),
        cli_subset(input, &["--gids=1,2,3"])
    );
    let missing = format!(
        "--text-file={}",
        directory.path().join("missing.txt").display()
    );
    let result = Command::new(env!("CARGO_BIN_EXE_skera"))
        .args(["--path", input, &missing, "--output-file"])
        .arg(output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Failed reading selector file"));
}

#[test]
fn wildcard_lines_in_selector_files_keep_the_complete_set() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("all.txt");
    std::fs::write(&file, "* # all\n1\n").unwrap();
    let option = format!("--gids-file={}", file.display());
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    assert_eq!(
        cli_subset(input, &[&option]),
        cli_subset(input, &["--gids=*"])
    );
    std::fs::write(&file, "*\n0061\n").unwrap();
    let option = format!("--unicodes-file={}", file.display());
    assert_eq!(
        cli_subset(input, &[&option]),
        cli_subset(input, &["--unicodes=*"])
    );
    std::fs::write(&file, "*\na\n").unwrap();
    let option = format!("--text-file={}", file.display());
    assert_eq!(
        cli_subset(input, &[&option]),
        cli_subset(input, &["--unicodes=*"])
    );
}

#[test]
fn glyph_and_unicode_operations_add_remove_replace_and_handle_wildcards() {
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    for (options, expected) in [
        (
            vec!["--text=abc", "--text-=b", "--unicodes+=0062"],
            "--text=abc",
        ),
        (
            vec!["--text=a", "--text+=b", "--text+=c", "--unicodes-=62"],
            "--text=ac",
        ),
        (vec!["--text+=a", "--unicodes=62", "--text+=c"], "--text=bc"),
        (vec!["--unicodes=61-63", "--unicodes-=61-62"], "--text=c"),
        (vec!["--text=*", "--text-=b"], "--text=ac"),
        (
            vec!["--unicodes=61", "--text+=*", "--unicodes-=62"],
            "--text=ac",
        ),
        (vec!["--text=abc", "--text-=*"], "--text="),
        (
            vec!["--unicodes=*", "--unicodes-=*", "--text+=b"],
            "--text=b",
        ),
        (vec!["--gids=1", "--gids+=2-3", "--gids-=2"], "--gids=1,3"),
        (
            vec!["--glyphs=uni0061", "--glyphs+=uni0062", "--glyphs-=uni0061"],
            "--gids=2",
        ),
        (
            vec![
                "--glyphs=uni0061",
                "--gids+=2",
                "--glyphs+=uni0063",
                "--gids-=1",
            ],
            "--gids=2,3",
        ),
        (
            vec!["--gids+=1", "--glyphs=uni0062", "--gids+=3"],
            "--gids=2,3",
        ),
        (vec!["--gids=*", "--glyphs-=uni0062"], "--gids=0,1,3"),
        (vec!["--gids=1", "--glyphs+=*", "--gids-=2"], "--gids=0,1,3"),
        (vec!["--gids=*", "--glyphs-=*", "--gids+=3"], "--gids=3"),
        (vec!["--glyphs=*", "--gids-=*"], "--gids="),
        (
            vec!["--keep-everything", "--gids-=*", "--text-=b"],
            "--unicodes=61,63",
        ),
    ] {
        let expected_options = if options.contains(&"--keep-everything") {
            vec!["--keep-everything", "--gids=", expected]
        } else {
            vec![expected]
        };
        assert_eq!(
            cli_subset(input, &options),
            cli_subset(input, &expected_options),
            "{options:?}"
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("text.txt");
    std::fs::write(&file, "b\n").unwrap();
    let option = format!("--text-file={}", file.display());
    assert_eq!(
        cli_subset(input, &["--text=a", &option, "--text-=a", "--text+=c"]),
        cli_subset(input, &["--text=bc"])
    );
}

#[test]
fn metadata_operations_filter_actual_names_layout_items_and_tables() {
    use write_fonts::{
        from_obj::ToOwnedTable,
        read::{types::NameId, TableProvider},
        tables::name::{Name, NameRecord},
        types::Tag,
        FontBuilder,
    };
    let source = std::fs::read("test-data/fonts/layout-feature-dedup.ttf").unwrap();
    let font = FontRef::new(&source).unwrap();
    let mut name: Name = font.name().unwrap().to_owned_table();
    name.name_record.extend([
        NameRecord::new(
            3,
            1,
            0x409,
            NameId::new(500),
            "English extra".to_owned().into(),
        ),
        NameRecord::new(
            3,
            1,
            0x40c,
            NameId::new(501),
            "French extra".to_owned().into(),
        ),
    ]);
    name.name_record.sort();
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
    }
    builder.add_table(&name).unwrap();
    builder.add_raw(Tag::new(b"TEST"), b"opaque table");
    builder.add_raw(Tag::new(b"DSIG"), &[0, 0, 0, 1, 0, 0, 0, 0]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.ttf");
    std::fs::write(&path, builder.build()).unwrap();
    let input = path.to_str().unwrap();
    for (options, expected) in [
        (
            vec!["--name-IDs+=500", "--name-IDs-=1"],
            vec!["--name-IDs=0,2,3,4,5,6,500"],
        ),
        (
            vec!["--name-IDs=1", "--name-IDs+=500", "--name-IDs-=1"],
            vec!["--name-IDs=500"],
        ),
        (
            vec!["--name-IDs=500", "--name-IDs=501"],
            vec!["--name-IDs=501"],
        ),
        (
            vec!["--name-IDs=*", "--name-IDs-=*", "--name-IDs+=500"],
            vec!["--name-IDs=500"],
        ),
        (
            vec![
                "--name-IDs+=501",
                "--name-languages+=1036",
                "--name-languages-=1033",
            ],
            vec!["--name-IDs=0,1,2,3,4,5,6,501", "--name-languages=1036"],
        ),
        (
            vec!["--name-languages=*", "--name-languages-=*"],
            vec!["--name-languages="],
        ),
        (
            vec!["--name-languages=1036", "--name-languages=1033"],
            vec!["--name-languages=1033"],
        ),
        (
            vec![
                "--layout-features=liga",
                "--layout-features+=salt",
                "--layout-features-=liga",
            ],
            vec!["--layout-features=salt"],
        ),
        (
            vec!["--layout-features=liga", "--layout-features=salt"],
            vec!["--layout-features=salt"],
        ),
        (
            vec!["--layout-features=*", "--layout-features-=*"],
            vec!["--layout-features="],
        ),
        (
            vec!["--layout-features=", "--layout-features+=*"],
            vec!["--layout-features=*"],
        ),
        (
            vec!["--layout-scripts-=*", "--layout-scripts+=latn"],
            vec!["--layout-scripts=latn"],
        ),
        (
            vec![
                "--layout-scripts=latn",
                "--layout-scripts+=DFLT",
                "--layout-scripts-=latn",
            ],
            vec!["--layout-scripts=DFLT"],
        ),
        (
            vec!["--layout-scripts=latn", "--layout-scripts=DFLT"],
            vec!["--layout-scripts=DFLT"],
        ),
        (
            vec![
                "--drop-tables=DSIG",
                "--drop-tables+=TEST",
                "--drop-tables-=DSIG",
            ],
            vec!["--drop-tables=TEST"],
        ),
        (
            vec!["--drop-tables=DSIG", "--drop-tables=TEST"],
            vec!["--drop-tables=TEST"],
        ),
        (
            vec!["--drop-tables-=*", "--drop-tables+=DSIG"],
            vec!["--drop-tables=DSIG"],
        ),
    ] {
        let mut actual = vec!["--gids=*", "--passthrough-tables"];
        actual.extend(options.iter().copied());
        let mut reference = vec!["--gids=*", "--passthrough-tables"];
        reference.extend(expected);
        assert_eq!(
            cli_subset(input, &actual),
            cli_subset(input, &reference),
            "{options:?}"
        );
    }
    let bytes = cli_subset(
        input,
        &[
            "--gids=*",
            "--name-IDs+=501",
            "--name-languages+=1036",
            "--drop-tables-=DSIG",
            "--passthrough-tables",
        ],
    );
    let output = FontRef::new(&bytes).unwrap();
    assert!(output
        .name()
        .unwrap()
        .name_record()
        .iter()
        .any(|record| record.name_id() == NameId::new(501) && record.language_id() == 0x40c));
    assert!(output.data_for_tag(Tag::new(b"DSIG")).is_some());
    assert!(output.data_for_tag(Tag::new(b"TEST")).is_some());
    let bytes = cli_subset(
        input,
        &["--gids=*", "--layout-features-=*", "--layout-scripts-=*"],
    );
    let output = FontRef::new(&bytes).unwrap();
    if let Ok(gsub) = output.gsub() {
        assert_eq!(gsub.feature_list().unwrap().feature_count(), 0);
        assert_eq!(gsub.script_list().unwrap().script_count(), 0);
        assert_eq!(gsub.lookup_list().unwrap().lookup_count(), 0);
    }
    if let Ok(gpos) = output.gpos() {
        assert_eq!(gpos.feature_list().unwrap().feature_count(), 0);
        assert_eq!(gpos.script_list().unwrap().script_count(), 0);
        assert_eq!(gpos.lookup_list().unwrap().lookup_count(), 0);
    }
}

#[test]
fn keep_everything_resets_selections_and_flags_in_command_line_order() {
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    for (options, expected) in [
        (
            vec!["--no-hinting", "--keep-everything"],
            vec!["--keep-everything"],
        ),
        (
            vec!["--retain-gids", "--keep-everything", "--gids=", "--text=c"],
            vec!["--keep-everything", "--gids=", "--text=c"],
        ),
        (
            vec!["--keep-everything", "--no-hinting"],
            vec!["--keep-everything", "--no-hinting"],
        ),
        (
            vec!["--no-hinting", "--keep-everything", "--no-hinting"],
            vec!["--keep-everything", "--no-hinting"],
        ),
        (
            vec![
                "--gids=1",
                "--unicodes=61",
                "--name-IDs=",
                "--name-languages=",
                "--layout-features=",
                "--layout-scripts=",
                "--drop-tables=name",
                "--keep-everything",
            ],
            vec!["--keep-everything"],
        ),
        (
            vec![
                "--keep-everything",
                "--gids=1",
                "--unicodes=61",
                "--name-IDs=",
                "--no-hinting",
                "--keep-everything",
            ],
            vec!["--keep-everything"],
        ),
        (
            vec![
                "--keep-everything",
                "--gids=1",
                "--unicodes=61",
                "--keep-everything",
                "--gids-=*",
                "--unicodes-=61-62",
            ],
            vec!["--keep-everything", "--gids=", "--text=c"],
        ),
    ] {
        assert_eq!(
            cli_subset(input, &options),
            cli_subset(input, &expected),
            "{options:?}"
        );
    }
    // A reset also restores layout selections excluded earlier.
    let input = "test-data/fonts/layout-feature-dedup.ttf";
    assert_eq!(
        cli_subset(
            input,
            &[
                "--layout-features=",
                "--layout-scripts=",
                "--keep-everything"
            ]
        ),
        cli_subset(input, &["--keep-everything"])
    );
    // The CFF2 downgrade flag follows the same reset semantics as other flags.
    let input = "test-data/fonts/AdobeVFPrototype.otf";
    assert_eq!(
        cli_subset(
            input,
            &[
                "--downgrade-cff2",
                "--keep-everything",
                "--instance=wght=650,CNTR=drop"
            ]
        ),
        cli_subset(
            input,
            &["--keep-everything", "--instance=wght=650,CNTR=drop"]
        )
    );
}

#[test]
fn variation_options_accept_open_bounds_wildcards_and_repeated_requests() {
    let input = "test-data/fonts/AdobeVFPrototype.otf";
    for (options, expected) in [
        (
            vec!["--instance=wght=650", "--variations=CNTR=drop"],
            "wght=650,CNTR=drop",
        ),
        (
            vec!["--instance=wght=650", "--instance=wght=700"],
            "wght=700",
        ),
        (vec!["--instance=wght=650 CNTR=drop"], "wght=650,CNTR=drop"),
        (vec!["--instance=*=drop,wght=650"], "wght=650,CNTR=drop"),
        (vec!["--instance=wght=650,*=drop"], "wght=drop,CNTR=drop"),
        (vec!["--instance=wght=300::700"], "wght=300:700"),
        (vec!["--instance=wght=-100:2000"], "wght=:"),
    ] {
        let mut options = options;
        options.push("--gids=0-3");
        assert_eq!(
            cli_subset(input, &options),
            cli_subset(input, &["--gids=0-3", &format!("--instance={expected}")]),
            "{options:?}"
        );
    }
}

#[test]
fn varc_preliminary_subsets_keep_inputs_until_instancing_and_selection_finish() {
    use write_fonts::{
        from_obj::ToOwnedTable,
        read::TableProvider,
        tables::{glyf::Glyph, hmtx::Hmtx},
        types::{GlyphId, Tag},
    };
    let input = "test-data/fonts/varc-unrelated-axis.ttf";
    let common = ["--gids=3", "--instance=*=drop,DUMY=0.5", "--notdef-outline"];
    let reference = cli_subset(input, &common);
    let expected = FontRef::new(&reference).unwrap();
    let outlines = |font: &FontRef| {
        let (glyf, loca) = (font.glyf().unwrap(), font.loca(None).unwrap());
        (0..font.maxp().unwrap().num_glyphs())
            .map(|gid| {
                loca.get(GlyphId::from(gid), &glyf)
                    .unwrap()
                    .into_glyph()
                    .map(|glyph| {
                        let glyph: Glyph = glyph.to_owned_table();
                        glyph
                    })
            })
            .collect::<Vec<_>>()
    };
    for tag in [
        "fvar", "gvar", "avar", "cvar", "HVAR", "VVAR", "MVAR", "GDEF",
    ] {
        let mut options = common.to_vec();
        let drop = format!("--drop-tables={tag}");
        options.push(&drop);
        let bytes = cli_subset(input, &options);
        let actual = FontRef::new(&bytes).unwrap();
        assert!(actual
            .data_for_tag(Tag::new(tag.as_bytes().try_into().unwrap()))
            .is_none());
        assert_eq!(outlines(&actual), outlines(&expected), "{tag}");
        let actual_metrics: Hmtx = actual.hmtx().unwrap().to_owned_table();
        let expected_metrics: Hmtx = expected.hmtx().unwrap().to_owned_table();
        assert_eq!(actual_metrics, expected_metrics, "{tag}");
    }
    // Removing cmap should retain glyphs selected through the original cmap.
    let common = ["--text=각", "--instance=DUMY=0.5", "--notdef-outline"];
    let reference = cli_subset(input, &common);
    let expected = FontRef::new(&reference).unwrap();
    let mut options = common.to_vec();
    options.push("--drop-tables=cmap");
    let bytes = cli_subset(input, &options);
    let actual = FontRef::new(&bytes).unwrap();
    assert!(actual.cmap().is_err());
    assert_eq!(outlines(&actual), outlines(&expected));
    assert_eq!(
        actual.maxp().unwrap().num_glyphs(),
        expected.maxp().unwrap().num_glyphs()
    );
}

#[test]
fn glyph_maps_include_glyphs_in_order_and_accumulate_repeated_options() {
    use write_fonts::read::TableProvider;
    let input = "test-data/fonts/Roboto-Regular.abc.ttf";
    for (options, expected) in [
        (vec!["--gid-map=1:5", "--gids=2"], vec!["--gids=2"]),
        (vec!["--gid-map=1:5", "--gids-=1"], vec!["--gids="]),
        (
            vec!["--gids=2", "--gid-map=1:5"],
            vec!["--gid-map=1:5", "--gids=1,2"],
        ),
        (
            vec![
                "--gid-map=1:5",
                "--keep-everything",
                "--gids-=1",
                "--unicodes-=61",
            ],
            vec!["--keep-everything", "--gids=2,3", "--unicodes="],
        ),
        (
            vec!["--gid-map=1:5", "--gid-map=2:7"],
            vec!["--gid-map=1:5,2:7"],
        ),
        (vec!["--gid-map=1:5 2:7,"], vec!["--gid-map=1:5,2:7"]),
        (
            vec!["--gid-map=1:5", "--glyph-map=1:7"],
            vec!["--gid-map=1:7"],
        ),
        (
            vec!["--gid-map=1:5", "--gid-map=", "--glyph-map=2:7"],
            vec!["--gid-map=1:5,2:7"],
        ),
        (
            vec!["--gid-map=1:5", "--glyphs=uni0062", "--gid-map=2:7"],
            vec!["--gid-map=2:7"],
        ),
    ] {
        assert_eq!(
            cli_subset(input, &options),
            cli_subset(input, &expected),
            "{options:?}"
        );
    }
    let bytes = cli_subset(input, &["--gid-map=1:5", "--gids=2"]);
    assert_eq!(
        FontRef::new(&bytes).unwrap().maxp().unwrap().num_glyphs(),
        2
    );
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("glyphs.txt");
    std::fs::write(&file, "gid2\n").unwrap();
    let option = format!("--glyphs-file={}", file.display());
    assert_eq!(
        cli_subset(input, &["--gid-map=1:5", &option, "--gids-=1"]),
        cli_subset(input, &["--gids=2"])
    );
}
