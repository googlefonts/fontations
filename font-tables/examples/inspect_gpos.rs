use font_tables::{FontData, FontRef, ReadError, TableProvider};

fn main() {
    let path = std::env::args().nth(1).expect("missing path argument");
    let bytes = std::fs::read(path).unwrap();
    let data = FontData::new(&bytes);
    let font = FontRef::new(data).unwrap();
    let gpos = match font.gpos() {
        Ok(gpos) => gpos,
        Err(ReadError::TableIsMissing(_)) => {
            println!("No GPOS table found");
            return;
        }
        Err(e) => {
            eprintln!("Failed to parse GPOS: '{e}'");
            std::process::exit(1);
        }
    };

    println!("{:#?}", &gpos);
    println!("GPOS {:?}", gpos.version());

    let script_list = gpos.script_list().expect("missing script list");
    let feature_list = gpos.feature_list().expect("missing feature list");
    let lookup_list = gpos.lookup_list().expect("missing lookup_list");

    println!("{} scripts:", script_list.script_count());
    for record in script_list.script_records() {
        let table = record
            .script(script_list.offset_data())
            .expect("missing scrpt table");
        println!(
            "script '{}', {} langs",
            record.script_tag(),
            table.lang_sys_count()
        );
    }

    println!("{} features:", feature_list.feature_count());

    for record in feature_list.feature_records() {
        let table = record.feature(feature_list.offset_data()).unwrap();
        println!(
            "feature '{}', {} lookups",
            record.feature_tag(),
            table.lookup_index_count()
        );
    }

    println!("{} lookups", lookup_list.lookup_count());
    for (i, lookup) in lookup_list.lookups().enumerate() {
        let lookup = lookup.unwrap();
        println!(
            "lookup {i}, type {}, {} subtables",
            lookup.lookup_type(),
            lookup.sub_table_count()
        );
    }
}
