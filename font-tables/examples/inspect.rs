//! Print the contents of font tables.
//!
//! This accepts command line arguments similar to what is present in ttx,
//! although it does not produce xml output.

use std::{collections::HashSet, str::FromStr};

use font_tables::{FontData, FontRef, TableProvider};
use font_types::Tag;

fn main() -> Result<(), Error> {
    let args = match flags::Args::from_env() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let bytes = std::fs::read(&args.input).unwrap();
    let data = FontData::new(&bytes);
    let font = FontRef::new(data).unwrap();
    if args.list {
        list_tables(&font);
        return Ok(());
    }

    let filter = TableFilter::from_args(&args)?;
    Ok(print_tables(&font, &filter))
}

fn list_tables(font: &FontRef) {
    println!("Tag  Offset  Length  Checksum");
    println!("-------------------------------");
    for record in font.table_directory.table_records() {
        println!(
            "{} 0x{:04X} {:8} 0x{:08X} ",
            record.tag(),
            record.offset().to_u32(),
            record.length(),
            record.checksum()
        );
    }
}

fn print_tables(font: &FontRef, filter: &TableFilter) {
    for tag in font
        .table_directory
        .table_records()
        .iter()
        .map(|rec| rec.tag())
        .filter(|tag| filter.should_print(*tag))
    {
        print_table(font, tag)
    }
}

fn print_table(font: &FontRef, tag: Tag) {
    match tag {
        font_tables::tables::gpos::TAG => println!("{tag}: {:#?}", font.gpos().unwrap()),
        font_tables::tables::cmap::TAG => println!("{tag}: {:#?}", font.cmap().unwrap()),
        font_tables::tables::gdef::TAG => println!("{tag}: {:#?}", font.gdef().unwrap()),
        font_tables::tables::glyf::TAG => println!("{tag}: {:#?}", font.glyf().unwrap()),
        font_tables::tables::head::TAG => println!("{tag}: {:#?}", font.head().unwrap()),
        font_tables::tables::hhea::TAG => println!("{tag}: {:#?}", font.hhea().unwrap()),
        font_tables::tables::hmtx::TAG => println!("{tag}: {:#?}", font.hmtx().unwrap()),
        //font_tables::tables::loca::TAG => println!("{tag}: {:#?}", font.loca().unwrap()),
        font_tables::tables::maxp::TAG => println!("{tag}: {:#?}", font.maxp().unwrap()),
        font_tables::tables::name::TAG => println!("{tag}: {:#?}", font.name().unwrap()),
        font_tables::tables::post::TAG => println!("{tag}: {:#?}", font.post().unwrap()),
        _ => println!("unknown tag {tag}"),
    }
}

enum TableFilter {
    All,
    Include(HashSet<Tag>),
    Exclude(HashSet<Tag>),
}

impl TableFilter {
    fn from_args(args: &flags::Args) -> Result<Self, Error> {
        if args.tables.is_some() && args.exclude.is_some() {
            return Err(Error::new("pass only one of --tables and --exclude"));
        }
        if let Some(tags) = &args.tables {
            make_tag_set(tags).map(TableFilter::Include)
        } else if let Some(tags) = &args.exclude {
            make_tag_set(tags).map(TableFilter::Exclude)
        } else {
            Ok(TableFilter::All)
        }
    }

    fn should_print(&self, tag: Tag) -> bool {
        match self {
            TableFilter::All => true,
            TableFilter::Include(tags) => tags.contains(&tag),
            TableFilter::Exclude(tags) => !tags.contains(&tag),
        }
    }
}

fn make_tag_set(inp: &str) -> Result<HashSet<Tag>, Error> {
    inp.split(' ')
        .map(|raw| match Tag::from_str(raw) {
            Ok(tag) => Ok(tag),
            Err(e) => Err(Error(format!(
                "Invalid tag '{}': {e}",
                raw.escape_default()
            ))),
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Error(String);

impl Error {
    fn new(t: impl std::fmt::Display) -> Self {
        Self(t.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

mod flags {
    use std::path::PathBuf;

    xflags::xflags! {
        /// Generate font table representations
        cmd args
            required input: PathBuf
            {
                optional -l, --list
                optional -t, --tables include: String
                optional -x, --exclude exclude: String
            }

    }
}
