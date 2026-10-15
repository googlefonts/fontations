//! Explicit naming records override normal selection filters.
use skera::{subset_font, Plan, SubsetError, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use write_fonts::read::{
    collections::IntSet, tables::name::Name, types::NameId, types::Tag, FontData, FontRead,
    FontRef, TableProvider,
};

const FONT: &[u8] = include_bytes!("../test-data/fonts/Roboto-Regular.ttf");
type Key = (u16, u16, u16, u16);

fn plan(font: &FontRef, drop: &IntSet<Tag>) -> Plan {
    Plan::new(
        &IntSet::empty(),
        &IntSet::from_iter(0x61..=0x63),
        font,
        SubsetFlags::default(),
        drop,
        &IntSet::all(),
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        &IntSet::from_iter((0..=6).map(NameId::new)),
        &IntSet::from_iter([0x409]),
    )
}

fn records(name: &Name) -> Vec<(Key, Vec<u8>)> {
    name.name_record()
        .iter()
        .map(|r| {
            let start = r.string_offset().to_u32() as usize;
            let bytes = name.string_data().as_bytes()[start..start + r.length() as usize].to_vec();
            (
                (
                    r.platform_id(),
                    r.encoding_id(),
                    r.language_id(),
                    r.name_id().to_u16(),
                ),
                bytes,
            )
        })
        .collect()
}

fn overrides(plan: &mut Plan) {
    for (id, platform, encoding, language, text) in [
        (1, 3, 1, 0x409, Some("Renamed Café 😀")),
        (6, 0, 3, 0, Some("Renamed-Regular")),
        (300, 3, 1, 0x410, Some("Nome italiano")),
        (1, 1, 0, 0, Some("ASCII family")),
        (4, 3, 1, 0x409, None),
        (5, 3, 1, 0x409, Some("")),
        (300, 3, 1, 0x410, Some("Nome finale")),
        (301, 0, 4, 0, Some("Embedded\0NUL")),
    ] {
        plan.override_name_table(NameId::new(id), platform, encoding, language, text)
            .unwrap();
    }
}

#[test]
fn replacements_insertions_and_removals_match_harfbuzz() {
    let font = FontRef::new(FONT).unwrap();
    let mut plan = plan(&font, &IntSet::empty());
    let original = records(&font.name().unwrap());
    for key in [(3, 1, 0x409, 4), (3, 1, 0x409, 5)] {
        assert!(original.iter().any(|r| r.0 == key));
    }
    overrides(&mut plan);
    let out = subset_font(&font, &plan).unwrap();
    let out = FontRef::new(&out).unwrap();
    let name = out.name().unwrap();
    let expected = Name::read(FontData::new(include_bytes!(
        "../test-data/expected/name-overrides/name.bin"
    )))
    .unwrap();
    assert_eq!(records(&name), records(&expected));
    let actual = records(&name);
    for key in [(3, 1, 0x409, 4), (3, 1, 0x409, 5)] {
        assert!(!actual.iter().any(|r| r.0 == key));
    }
    assert_eq!(
        actual.iter().find(|r| r.0 == (1, 0, 0, 1)).unwrap().1,
        b"ASCII family"
    );
    for (key, text) in [
        ((3, 1, 0x409, 1), "Renamed Café 😀"),
        ((3, 1, 0x410, 300), "Nome finale"),
        ((0, 4, 0, 301), "Embedded\0NUL"),
    ] {
        let bytes: Vec<_> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert_eq!(actual.iter().find(|r| r.0 == key).unwrap().1, bytes);
    }
}

#[test]
fn rejected_overrides_preserve_previous_requests_and_large_names_fit() {
    let font = FontRef::new(FONT).unwrap();
    let mut plan = plan(&font, &IntSet::empty());
    plan.override_name_table(NameId::new(1), 1, 0, 0, Some("ASCII"))
        .unwrap();
    assert!(matches!(
        plan.override_name_table(NameId::new(1), 1, 0, 0, Some("Café")),
        Err(SubsetError::InvalidNameOverride(_))
    ));
    let large = "x".repeat(32767);
    plan.override_name_table(NameId::new(1), 3, 1, 0x409, Some(&large))
        .unwrap();
    assert!(matches!(
        plan.override_name_table(NameId::new(1), 3, 1, 0x409, Some(&"x".repeat(32768))),
        Err(SubsetError::InvalidNameOverride(_))
    ));
    let out = subset_font(&font, &plan).unwrap();
    let out = FontRef::new(&out).unwrap();
    let actual = records(&out.name().unwrap());
    assert_eq!(
        actual.iter().find(|r| r.0 == (1, 0, 0, 1)).unwrap().1,
        b"ASCII"
    );
    assert_eq!(
        actual.iter().find(|r| r.0 == (3, 1, 0x409, 1)).unwrap().1,
        large
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()
    );
}

#[test]
fn dropped_and_passthrough_tables_take_precedence() {
    let font = FontRef::new(FONT).unwrap();
    let tag = Tag::new(b"name");
    let mut drop = plan(&font, &IntSet::from_iter([tag]));
    overrides(&mut drop);
    let out = subset_font(&font, &drop).unwrap();
    assert!(FontRef::new(&out).unwrap().name().is_err());
    let mut passthrough = plan(&font, &IntSet::empty());
    overrides(&mut passthrough);
    passthrough.set_no_subset_tables(&IntSet::from_iter([tag]));
    let out = subset_font(&font, &passthrough).unwrap();
    assert_eq!(
        FontRef::new(&out)
            .unwrap()
            .data_for_tag(tag)
            .unwrap()
            .as_bytes(),
        font.data_for_tag(tag).unwrap().as_bytes()
    );
}
