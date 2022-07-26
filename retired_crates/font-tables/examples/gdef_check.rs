//! Comparing roundtriped GDEF tables

use font_tables::{
    compile::ToOwnedTable,
    layout::{ClassDef, CoverageTable},
    tables::{self, gdef::MarkGlyphSets, TableProvider},
    FontRef,
};
use font_types::{FontRead, OffsetHost};

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let gdef = font.gdef().expect("no GDEF table found");
    print_gdef_info(&gdef);
    round_trip(&gdef);
}

fn print_gdef_info(gdef: &tables::gdef::Gdef) {
    println!(
        "\nGDEF version {}.{}",
        gdef.major_version(),
        gdef.minor_version()
    );
    if let Some(class_def) = gdef.glyph_class_def() {
        let format = match class_def {
            ClassDef::Format1(_) => 1,
            ClassDef::Format2(_) => 2,
        };
        println!("   ClassDef format {}", format);
    }

    if let Some(attach_list) = gdef.attach_list() {
        println!("  AttachList ({} glyphs)", attach_list.glyph_count());
    }

    if let Some(lig_caret_list) = gdef.lig_caret_list() {
        println!(
            "  LigCaretList ({} glyphs)",
            lig_caret_list.lig_glyph_count()
        );
    }

    if let Some(class_def) = gdef.mark_attach_class_def() {
        let format = match class_def {
            ClassDef::Format1(_) => 1,
            ClassDef::Format2(_) => 2,
        };
        println!("   MarkAttach ClassDef format {}", format);
    }

    if let Some(glyph_sets) = gdef.mark_glyph_sets_def() {
        println!(
            "  MarkGlyphSets ({} glyphs)",
            glyph_sets.mark_glyph_set_count()
        );
    }
}

fn round_trip(gdef: &tables::gdef::Gdef) {
    let owned = gdef.to_owned_table().unwrap();
    //if let Some(gclass) = &owned.glyph_class_def {
    //let our_bytes = font_tables::compile::dump_table(gclass);
    //let orig = gdef.bytes_at_offset(gdef.glyph_class_def_offset());
    //font_tables::assert_hex_eq!(&our_bytes, &orig[..our_bytes.len()]);
    //}
    let bytes = font_tables::compile::dump_table(&owned);
    println!("bytes: {}", bytes.len());
    let ours = tables::gdef::Gdef::read(&bytes).unwrap();

    println!(
        "{:?} {:?}",
        gdef.glyph_class_def_offset(),
        ours.glyph_class_def_offset()
    );
    println!(
        "{:?} {:?}",
        gdef.attach_list_offset(),
        ours.attach_list_offset()
    );
    println!(
        "{:?} {:?}",
        gdef.lig_caret_list_offset(),
        ours.lig_caret_list_offset()
    );
    println!(
        "{:?} {:?}",
        gdef.mark_attach_class_def_offset(),
        ours.mark_attach_class_def_offset()
    );
    println!(
        "{:?} {:?}",
        gdef.mark_glyph_sets_def_offset(),
        ours.mark_glyph_sets_def_offset()
    );

    if let Some(class_def) = gdef.glyph_class_def() {
        let ours = ours.glyph_class_def().unwrap();
        let one = class_def.iter().collect::<Vec<_>>();
        let two = class_def.iter().collect::<Vec<_>>();
        if one != two {
            println!("class_def mismatch\n{one:?}\n{two:?}");
        }
    }

    if let Some(sets) = gdef.mark_glyph_sets_def() {
        inspect_mark_glyph_sets(&sets);
        inspect_mark_glyph_sets(&ours.mark_glyph_sets_def().unwrap());
    }
    font_tables::assert_hex_eq!(&gdef.bytes()[..], &bytes[..]);
    //inspect_mark_glyph_sets(&gdef.mark_glyph_sets_def_offset)
}

fn inspect_mark_glyph_sets(sets: &MarkGlyphSets) {
    println!("{}", sets.mark_glyph_set_count());
    for offset in sets.coverage_offsets() {
        let table: CoverageTable = sets.resolve_offset(offset.get()).unwrap();
        println!("{:?}", offset.get());
        let glyphs = table.iter().collect::<Vec<_>>();
        match table {
            CoverageTable::Format1(_) => println!("format 1, {glyphs:?}"),
            CoverageTable::Format2(table) => {
                println!("format 2, {glyphs:?}",);
                for record in table.range_records() {
                    dbg!(record);
                }
            }
        }
        //println!("{:?}: {}", offset.get(), table.format() )
    }
}
