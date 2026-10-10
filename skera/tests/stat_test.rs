//! STAT axis value filtering follows HarfBuzz and retains STAT axis indices.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontRef, TableProvider},
    tables::{
        name::{Name, NameRecord},
        stat::*,
    },
    types::{Fixed, NameId, Tag},
    FontBuilder,
};

fn stat() -> Stat {
    let flags = AxisValueTableFlags::ELIDABLE_AXIS_VALUE_NAME;
    Stat::new(
        vec![
            AxisRecord::new(Tag::new(b"wdth"), NameId::new(257), 2),
            AxisRecord::new(Tag::new(b"wght"), NameId::new(256), 1),
            AxisRecord::new(Tag::new(b"slnt"), NameId::new(258), 3),
        ],
        vec![
            AxisValue::format_1(1, flags, NameId::new(300), Fixed::from_f64(200.)),
            AxisValue::format_2(
                1,
                flags,
                NameId::new(301),
                Fixed::from_f64(400.),
                Fixed::from_f64(200.),
                Fixed::from_f64(900.),
            ),
            AxisValue::format_3(
                1,
                flags,
                NameId::new(302),
                Fixed::from_f64(700.),
                Fixed::from_f64(900.),
            ),
            AxisValue::format_1(0, flags, NameId::new(303), Fixed::from_f64(75.)),
            AxisValue::format_1(0, flags, NameId::new(304), Fixed::from_f64(100.)),
            AxisValue::format_4(
                flags,
                NameId::new(305),
                vec![
                    AxisValueRecord::new(1, Fixed::from_f64(400.)),
                    AxisValueRecord::new(0, Fixed::from_f64(75.)),
                ],
            ),
            AxisValue::format_4(
                flags,
                NameId::new(306),
                vec![
                    AxisValueRecord::new(1, Fixed::from_f64(700.)),
                    AxisValueRecord::new(0, Fixed::from_f64(100.)),
                ],
            ),
            // This static style axis is absent from fvar.
            AxisValue::format_1(2, flags, NameId::new(307), Fixed::from_f64(20.)),
            // Unspecified axes are not filtered to their fvar ranges.
            AxisValue::format_1(0, flags, NameId::new(308), Fixed::from_f64(60.)),
        ],
        NameId::new(2),
    )
}

fn source(raw: Vec<u8>) -> Vec<u8> {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let mut builder = FontBuilder::new();
    for r in font.table_directory().table_records() {
        builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
    }
    let mut name: Name = font.name().unwrap().to_owned_table();
    name.name_record.retain(|record| {
        record.name_id != NameId::new(258) && !(300..=308).contains(&record.name_id.to_u16())
    });
    for id in [258].into_iter().chain(300..=308) {
        name.name_record.push(NameRecord::new(
            3,
            1,
            0x409,
            NameId::new(id),
            format!("Style {id}").into(),
        ));
    }
    name.name_record.sort();
    builder.add_table(&name).unwrap();
    builder.add_raw(Tag::new(b"STAT"), raw);
    builder.build()
}

fn names(font: &FontRef) -> Vec<u16> {
    font.stat()
        .unwrap()
        .offset_to_axis_values()
        .map(|a| {
            a.unwrap()
                .axis_values()
                .iter()
                .map(|v| v.unwrap().value_name_id().to_u16())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn all_axis_value_formats_filter_requested_ranges_and_keep_axis_records() {
    let original = stat();
    for minor in [1u16, 2] {
        let mut raw = write_fonts::dump_table(&original).unwrap();
        raw[2..4].copy_from_slice(&minor.to_be_bytes());
        let bytes = source(raw);
        for (limits, expected) in [
            ("wght=300:700", vec![301, 302, 303, 304, 305, 306, 307, 308]),
            ("wght=400", vec![301, 303, 304, 305, 307, 308]),
            ("wght=drop", vec![301, 303, 304, 305, 307, 308]),
            ("wght=700,wdth=75", vec![302, 303, 307]),
            ("wght=400,wdth=75", vec![301, 303, 305, 307]),
            ("wght=500,wdth=100", vec![304, 307]),
            ("wght=1000", vec![303, 304, 307, 308]),
            ("wght=300:700,wdth=75:90", vec![301, 302, 303, 305, 307]),
        ] {
            let bytes = instance_font(
                &FontRef::new(&bytes).unwrap(),
                &parse_axis_limits(limits).unwrap(),
            )
            .unwrap();
            let font = FontRef::new(&bytes).unwrap();
            let table = font.stat().unwrap();
            assert_eq!(table.version().minor, minor, "{limits}");
            assert_eq!(names(&font), expected, "{limits}");
            let actual: Stat = table.to_owned_table();
            assert_eq!(actual.design_axes, original.design_axes);
            assert_eq!(
                actual.elided_fallback_name_id,
                original.elided_fallback_name_id
            );
            let expected_values = original
                .offset_to_axis_values
                .as_ref()
                .unwrap()
                .iter()
                .filter(|v| {
                    expected.contains(
                        &match v.as_ref() {
                            AxisValue::Format1(v) => v.value_name_id,
                            AxisValue::Format2(v) => v.value_name_id,
                            AxisValue::Format3(v) => v.value_name_id,
                            AxisValue::Format4(v) => v.value_name_id,
                        }
                        .to_u16(),
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                actual.offset_to_axis_values.as_ref().unwrap(),
                &expected_values,
                "{limits}"
            );
        }
    }
}

#[test]
fn subsequent_subsetting_retains_names_of_surviving_values() {
    let bytes = source(write_fonts::dump_table(&stat()).unwrap());
    let bytes = instance_font(
        &FontRef::new(&bytes).unwrap(),
        &parse_axis_limits("wght=300:700").unwrap(),
    )
    .unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("1").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &(0..=6).map(NameId::new).collect(),
        &IntSet::all(),
    );
    let bytes = subset_font(&font, &plan).unwrap();
    let bytes = instance_font(
        &FontRef::new(&bytes).unwrap(),
        &parse_axis_limits("wght=400,wdth=75").unwrap(),
    )
    .unwrap();
    let font = FontRef::new(&bytes).unwrap();
    assert_eq!(names(&font), [301, 303, 305, 307]);
    assert_eq!(font.stat().unwrap().design_axis_count(), 3);
    // Rebuild the plan after the final instance to exercise name closure.
    let plan = Plan::new(
        &skera::populate_gids("1").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::default(),
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &(0..=6).map(NameId::new).collect(),
        &IntSet::all(),
    );
    let bytes = subset_font(&font, &plan).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let ids = font
        .name()
        .unwrap()
        .name_record()
        .iter()
        .map(|r| r.name_id())
        .collect::<IntSet<_>>();
    for id in 300..=308 {
        assert_eq!(
            ids.contains(NameId::new(id)),
            [301, 303, 305, 307].contains(&id)
        );
    }
    for id in [2, 256, 257, 258] {
        assert!(ids.contains(NameId::new(id)));
    }
}

#[test]
fn invalid_axis_indices_and_missing_value_arrays_are_rejected() {
    let mut table = stat();
    let AxisValue::Format1(value) = table.offset_to_axis_values.as_mut().unwrap()[0].as_mut()
    else {
        panic!()
    };
    value.axis_index = u16::MAX;
    let invalid_index = write_fonts::dump_table(&table).unwrap();
    let mut null_array = write_fonts::dump_table(&stat()).unwrap();
    null_array[14..18].fill(0);
    let mut truncated = write_fonts::dump_table(&stat()).unwrap();
    truncated.truncate(20);
    for raw in [invalid_index, null_array, truncated] {
        let bytes = source(raw);
        let font = FontRef::new(&bytes).unwrap();
        assert!(
            matches!(instance_font(&font, &parse_axis_limits("wght=400").unwrap()), Err(skera::SubsetError::SubsetTableError(t)) if t == Tag::new(b"STAT"))
        );
    }
}

#[test]
fn an_empty_value_array_keeps_the_design_axes_and_fallback_name() {
    let mut table = stat();
    table.offset_to_axis_values.as_mut().unwrap().truncate(1);
    let bytes = source(write_fonts::dump_table(&table).unwrap());
    let bytes = instance_font(
        &FontRef::new(&bytes).unwrap(),
        &parse_axis_limits("wght=400,wdth=75").unwrap(),
    )
    .unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let table = font.stat().unwrap();
    assert_eq!(table.design_axis_count(), 3);
    assert_eq!(table.axis_value_count(), 0);
    assert!(table.offset_to_axis_values().is_none());
    assert_eq!(table.elided_fallback_name_id(), Some(NameId::new(2)));
}

#[test]
fn large_axis_value_tables_are_repacked_when_short_offsets_overflow() {
    // The large format-4 value fits last in the source. Initial reverse graph
    // order places it before the format-1 values, requiring offset resolution.
    let count = 4000u16;
    let mut raw = Vec::new();
    raw.extend(0x00010002u32.to_be_bytes());
    raw.extend(8u16.to_be_bytes());
    raw.extend(count.to_be_bytes());
    raw.extend(20u32.to_be_bytes());
    raw.extend((count + 1).to_be_bytes());
    raw.extend((20 + u32::from(count) * 8).to_be_bytes());
    raw.extend(2u16.to_be_bytes());
    for i in 0..count {
        let tag = match i {
            0 => Tag::new(b"wght"),
            1 => Tag::new(b"wdth"),
            _ => Tag::new(format!("{i:04X}").as_bytes().try_into().unwrap()),
        };
        raw.extend(write_fonts::dump_table(&AxisRecord::new(tag, NameId::new(256), i)).unwrap());
    }
    let array_size = (count + 1) * 2;
    for i in 0..=count {
        raw.extend((array_size + i * 12).to_be_bytes());
    }
    for i in 0..count {
        raw.extend(
            write_fonts::dump_table(&AxisValue::format_1(
                0,
                AxisValueTableFlags::empty(),
                NameId::new(300 + i),
                Fixed::from_f64(400.),
            ))
            .unwrap(),
        );
    }
    raw.extend(
        write_fonts::dump_table(&AxisValue::format_4(
            AxisValueTableFlags::empty(),
            NameId::new(300),
            (0..count)
                .map(|i| AxisValueRecord::new(i, Fixed::from_f64(400.)))
                .collect(),
        ))
        .unwrap(),
    );
    let bytes = source(raw);
    let bytes = instance_font(
        &FontRef::new(&bytes).unwrap(),
        &parse_axis_limits("wght=400").unwrap(),
    )
    .unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let table = font.stat().unwrap();
    assert_eq!(table.design_axis_count(), count);
    assert_eq!(table.axis_value_count(), count + 1);
    let array = table.offset_to_axis_values().unwrap().unwrap();
    for (i, value) in array.axis_values().iter().enumerate() {
        match value.unwrap() {
            write_fonts::read::tables::stat::AxisValue::Format1(v) => {
                assert_eq!(v.value_name_id(), NameId::new(300 + i as u16))
            }
            write_fonts::read::tables::stat::AxisValue::Format4(v) => {
                assert_eq!(v.axis_values().len(), count as usize)
            }
            _ => panic!(),
        }
    }
}

#[test]
fn stat_tables_match_harfbuzz() {
    use write_fonts::read::{FontData, FontRead};
    let bytes = source(write_fonts::dump_table(&stat()).unwrap());
    for (limits, reference) in [
        ("wght=300:700", "wght=300-700"),
        ("wght=400", "wght=400"),
        ("wght=700,wdth=75", "wght=700_wdth=75"),
        ("wght=400,wdth=75", "wght=400_wdth=75"),
    ] {
        let expected_bytes =
            std::fs::read(format!("test-data/expected/stat/{reference}.bin")).unwrap();
        let expected: Stat =
            write_fonts::read::tables::stat::Stat::read(FontData::new(&expected_bytes))
                .unwrap()
                .to_owned_table();
        let bytes = instance_font(
            &FontRef::new(&bytes).unwrap(),
            &parse_axis_limits(limits).unwrap(),
        )
        .unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let actual: Stat = font.stat().unwrap().to_owned_table();
        assert_eq!(actual, expected, "{limits}");
    }
}
