//! Compact COLRv1 fields can overflow while retained axes bring them back.
use skera::{instance_font, parse_axis_limits};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{tables::variations::DeltaSetIndex, FontRef, TableProvider},
    tables::colr::{Colr, Paint},
    FontBuilder,
};

const SOURCE: &[u8] = include_bytes!("../test-data/fonts/colr-partial-overflow.ttf");
const CONSTANT: &[u8] = include_bytes!("../test-data/fonts/colr-constant-bias.ttf");

fn instance(bytes: &[u8], request: &str) -> Vec<u8> {
    instance_font(
        &FontRef::new(bytes).unwrap(),
        &parse_axis_limits(request).unwrap(),
    )
    .unwrap()
}

fn owned(bytes: &[u8]) -> Colr {
    FontRef::new(bytes)
        .unwrap()
        .colr()
        .unwrap()
        .to_owned_table()
}

#[test]
fn overflowing_defaults_survive_partial_instances_and_changed_defaults() {
    for explicit_map in [false, true] {
        let font = FontRef::new(SOURCE).unwrap();
        let mut colr = owned(SOURCE);
        if explicit_map {
            colr.var_index_map =
                Some((0u32..4).collect::<write_fonts::tables::variations::DeltaSetIndexMap>())
                    .into();
        }
        let mut builder = FontBuilder::new();
        for record in font.table_directory().table_records() {
            builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
        }
        builder.add_table(&colr).unwrap();
        let source = builder.build();
        let partial = instance(&source, "wght=1");
        let font = FontRef::new(&partial).unwrap();
        let table = font.colr().unwrap();
        let data = table.item_variation_store().unwrap().unwrap();
        assert_eq!(data.variation_region_list().unwrap().axis_count(), 1);
        assert!(data
            .variation_region_list()
            .unwrap()
            .variation_regions()
            .iter()
            .any(|r| {
                r.unwrap()
                    .region_axes()
                    .iter()
                    .all(|a| a.peak_coord().to_bits() == 0)
            }));
        let colr = owned(&partial);
        let Paint::VarSolid(solid) = colr
            .base_glyph_list
            .as_ref()
            .unwrap()
            .base_glyph_paint_records[0]
            .paint
            .as_ref()
        else {
            panic!();
        };
        let index = if let Some(map) = table.var_index_map().transpose().unwrap() {
            map.get(solid.var_index_base).unwrap()
        } else {
            DeltaSetIndex {
                outer: (solid.var_index_base >> 16) as u16,
                inner: solid.var_index_base as u16,
            }
        };
        assert_eq!(
            solid.alpha.to_f64()
                + data
                    .compute_delta(index, &[write_fonts::types::F2Dot14::ZERO])
                    .unwrap()
                    .to_f64()
                    / 16384.,
            3.
        );
        for width in [0.5, 0.75, 1.] {
            assert_eq!(
                owned(&instance(&partial, &format!("wdth={width}"))),
                owned(&instance(&source, &format!("wght=1,wdth={width}")))
            );
        }
        let rebased = instance(&source, "wght=0:0.5:1");
        for (weight, width) in [(0., 0.5), (0.5, 0.75), (1., 1.)] {
            let request = format!("wght={weight},wdth={width}");
            assert_eq!(
                owned(&instance(&rebased, &request)),
                owned(&instance(&source, &request))
            );
        }
    }
}

#[test]
fn constant_regions_apply_at_default_coordinates() {
    for request in ["wght=0,wdth=0", "wght=1,wdth=0"] {
        let colr = owned(&instance(CONSTANT, request));
        let Paint::Solid(solid) = colr
            .base_glyph_list
            .as_ref()
            .unwrap()
            .base_glyph_paint_records[0]
            .paint
            .as_ref()
        else {
            panic!();
        };
        assert_eq!(solid.alpha.to_f64(), 1.);
    }
    let partial = instance(CONSTANT, "wght=0");
    assert_eq!(
        owned(&instance(&partial, "wdth=0")),
        owned(&instance(CONSTANT, "wght=0,wdth=0"))
    );
}
