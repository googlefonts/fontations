//! BASE keeps its original header version, including a null 1.1 store offset.
//! References use an equivalent synthetic font and HarfBuzz c82300aefb.
use skera::{instance_font, parse_axis_limits, subset_font, Plan, SubsetFlags};
use write_fonts::read::tables::base as read;
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{collections::IntSet, FontData, FontRead, FontRef, TableProvider},
    tables::{base::*, layout::*, variations::*},
    types::{F2Dot14, MajorMinor, Tag},
    FontBuilder,
};

fn base(variable: bool, minor: u16) -> Vec<u8> {
    let axis = || {
        Axis::new(
            Some(BaseTagList::new(vec![Tag::new(b"ideo"), Tag::new(b"romn")])),
            BaseScriptList::new(vec![BaseScriptRecord::new(
                Tag::new(b"latn"),
                BaseScript::new(
                    Some(BaseValues::new(
                        1,
                        vec![
                            BaseCoord::format_1(100),
                            BaseCoord::format_3(
                                200,
                                Some(if variable {
                                    VariationIndex::new(0, 0).into()
                                } else {
                                    Device::new(10, 12, &[1, 0, -1]).into()
                                }),
                            ),
                        ],
                    )),
                    None,
                    vec![],
                ),
            )]),
        )
    };
    let mut table = Base::new(Some(axis()), Some(axis()));
    if minor > 0 {
        let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
        let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
        table.item_var_store = ItemVariationStore {
            variation_region_list: VariationRegionList::new(
                2,
                vec![VariationRegion::new(vec![pos, zero])],
            )
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: 1,
                word_delta_count: 1,
                region_indexes: vec![0],
                delta_sets: (-1i16).to_be_bytes().to_vec(),
            }
            .into()],
        }
        .into();
    }
    let mut raw = write_fonts::dump_table(&table).unwrap();
    if minor > 0 && !variable {
        raw[8..12].fill(0);
    }
    raw
}
fn font(raw: &[u8]) -> Vec<u8> {
    let bytes = std::fs::read("test-data/fonts/Roboto-Variable.composite.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let mut builder = FontBuilder::new();
    for r in font.table_directory().table_records() {
        if r.tag() != Tag::new(b"avar") {
            builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
        }
    }
    builder.add_raw(Tag::new(b"BASE"), raw);
    builder.build()
}
fn subset(bytes: &[u8]) -> Vec<u8> {
    let font = FontRef::new(bytes).unwrap();
    let plan = Plan::new(
        &skera::populate_gids("*").unwrap(),
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::all(),
    );
    subset_font(&font, &plan).unwrap()
}
fn check(bytes: &[u8], minor: u16, store: bool, expected: &Base) {
    let font = FontRef::new(bytes).unwrap();
    let base = font.base().unwrap();
    assert_eq!(base.version(), MajorMinor::new(1, minor));
    if minor > 0 {
        assert_eq!(base.item_var_store_offset().unwrap().is_null(), !store);
    }
    let actual: Base = base.to_owned_table();
    assert_eq!(actual, *expected);
}
#[test]
fn null_stores_preserve_versions_axes_coordinates_and_hint_devices() {
    for minor in [0, 1] {
        let raw = base(false, minor);
        let expected: Base = write_fonts::read::tables::base::Base::read(FontData::new(&raw))
            .unwrap()
            .to_owned_table();
        let source = font(&raw);
        check(&subset(&source), minor, false, &expected);
        for limits in ["wght=650,wdth=100", "wght=650", "wght=300:700"] {
            let bytes = instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits(limits).unwrap(),
            )
            .unwrap();
            check(&bytes, minor, false, &expected);
            check(&subset(&bytes), minor, false, &expected);
        }
    }
}
#[test]
fn instancing_a_base_store_keeps_a_null_version_1_1_store_field() {
    let source = font(&base(true, 1));
    for (limits, store) in [("wght=650,wdth=100", false), ("wght=650", true)] {
        let bytes = instance_font(
            &FontRef::new(&source).unwrap(),
            &parse_axis_limits(limits).unwrap(),
        )
        .unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let base = font.base().unwrap();
        assert_eq!(base.version(), MajorMinor::VERSION_1_1);
        assert_eq!(base.item_var_store_offset().unwrap().is_null(), !store);
        for axis in [base.horiz_axis(), base.vert_axis()] {
            let axis = axis.unwrap().unwrap();
            let list = axis.base_script_list().unwrap();
            let script = list.base_script_records()[0]
                .base_script(list.offset_data())
                .unwrap();
            let values = script.base_values().unwrap().unwrap();
            let read::BaseCoord::Format3(coord) = values.base_coords().get(1).unwrap() else {
                panic!()
            };
            assert_eq!(coord.coordinate(), 200);
            // Pinning weight makes this row constant even with width retained.
            assert!(coord.device().is_none());
        }
        assert_eq!(
            FontRef::new(&subset(&bytes))
                .unwrap()
                .base()
                .unwrap()
                .version(),
            MajorMinor::VERSION_1_1
        );
    }
}

#[test]
fn base_tables_match_harfbuzz_with_null_and_instantiated_stores() {
    for (variable, minor, operation) in [
        (false, 0, "subset"),
        (false, 1, "subset"),
        (true, 1, "full"),
    ] {
        let source = font(&base(variable, minor));
        let bytes = if variable {
            instance_font(
                &FontRef::new(&source).unwrap(),
                &parse_axis_limits("wght=650,wdth=100").unwrap(),
            )
            .unwrap()
        } else {
            source
        };
        let bytes = subset(&bytes);
        let reference = std::fs::read(format!(
            "test-data/expected/base/{minor}-{}-{operation}.bin",
            u8::from(variable)
        ))
        .unwrap();
        let expected = read::Base::read(FontData::new(&reference)).unwrap();
        assert_eq!(expected.version(), MajorMinor::new(1, minor));
        let expected: Base = expected.to_owned_table();
        check(&bytes, minor, false, &expected);
    }
}
