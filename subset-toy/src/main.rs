//! A toy subsetter, for illustration purposes.

use std::collections::BTreeSet;

use read_fonts::{FontRef, TableProvider, TopLevelTable};
use write_fonts::{from_obj::FromTableRef, tables::gpos::Gpos, FontBuilder};

use font_types::GlyphId;
use subset_toy::{Input, Subset};

fn main() {
    let args = match flags::Args::from_env() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let gids = populate_gids(&args.gids);
    let input = Input::from_gids(gids, args.retain_gids);

    let bytes = std::fs::read(&args.path).expect("no font file found");
    let font = FontRef::new(&bytes).expect("error reading font bytes");

    let plan = input.make_plan(&font);
    let gpos = font.gpos().expect("no gpos table found");
    let mut gpos_bytes = Vec::new();
    for _ in 0..args.runs.unwrap_or(1) {
        let mut gpos = Gpos::from_table_ref(&gpos);
        gpos.subset(&plan).expect("subsetting failed");
        gpos_bytes = write_fonts::dump_table(&gpos).unwrap();
    }

    let mut builder = FontBuilder::default();
    builder.add_raw(Gpos::TAG, gpos_bytes);

    // 'insert' was passed, we are going to copy our table into the passed font
    let bytes = if let Some(path) = args.insert {
        let bytes = std::fs::read(path).unwrap();
        let target = FontRef::new(&bytes).expect("failed to read insert font");
        builder.copy_missing_tables(target);
        builder.build()
    } else {
        builder.build()
    };

    std::fs::write(&args.out, bytes).unwrap();
}

fn populate_gids(gid_str: &str) -> BTreeSet<GlyphId> {
    let mut result = BTreeSet::new();
    for gid in gid_str.split(',') {
        if let Some((start, end)) = gid.split_once('-') {
            let start: u16 = start.parse().unwrap();
            let end: u16 = end.parse().unwrap();
            assert!(start <= end, "invalid gid range {gid}");
            result.extend((start..=end).map(GlyphId::new));
        } else {
            result.insert(GlyphId::new(gid.parse().unwrap()));
        }
    }
    result
}

mod flags {
    use std::path::PathBuf;

    xflags::xflags! {
        /// Generate font table representations
        cmd args
            required path: PathBuf
            {
                required -o, --out out: PathBuf
                required --gids gids: String
                optional --runs runs: usize
                optional --insert insert: PathBuf
                optional --retain-gids
            }

    }
}
