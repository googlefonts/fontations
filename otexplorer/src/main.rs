//! Print the contents of font tables.
//!
//! This accepts command line arguments similar to what is present in ttx,
//! although it does not produce xml output.

use std::{collections::HashSet, str::FromStr};

use font_types::Tag;
use read_fonts::{traversal::SomeTable, FontData, FontRef, ReadError, TableProvider};

mod print;
mod query;

use print::PrettyPrinter;
use query::Query;

fn main() -> Result<(), Error> {
    let args = flags::Args::from_env().map_err(|e| Error(e.to_string()))?;
    let bytes = std::fs::read(&args.input).unwrap();
    let data = FontData::new(&bytes);
    let font = FontRef::new(data).unwrap();
    if args.list {
        list_tables(&font);
        return Ok(());
    }

    if let Some(query) = &args.query {
        return query::print_query(&font, query).map_err(Error);
    }

    let filter = TableFilter::from_args(&args)?;
    print_tables(&font, &filter);
    Ok(())
}

fn list_tables(font: &FontRef) {
    println!("Tag  Offset  Length  Checksum");
    println!("-------------------------------");

    let offset_pad = get_offset_width(font);

    for record in font.table_directory.table_records() {
        println!(
            "{0} 0x{1:02$X} {3:8} 0x{4:08X} ",
            record.tag(),
            record.offset().to_u32(),
            offset_pad,
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

fn get_offset_width(font: &FontRef) -> usize {
    // pick how much padding we use for offsets based on the max offset in directory
    let max_off = font
        .table_directory
        .table_records()
        .iter()
        .map(|rec| rec.offset().to_u32())
        .max()
        .unwrap_or_default();
    hex_width(max_off)
}

fn hex_width(val: u32) -> usize {
    match val {
        0..=0xffff => 4usize,
        0x10000..=0xffff_ff => 6,
        0x1000000.. => 8,
    }
}

fn get_some_table<'a>(
    font: &FontRef<'a>,
    tag: Tag,
) -> Result<Box<dyn SomeTable<'a> + 'a>, ReadError> {
    match tag {
        read_fonts::tables::gpos::TAG => font.gpos().map(|x| Box::new(x) as _),
        read_fonts::tables::gsub::TAG => font.gsub().map(|x| Box::new(x) as _),
        read_fonts::tables::cmap::TAG => font.cmap().map(|x| Box::new(x) as _),
        read_fonts::tables::gdef::TAG => font.gdef().map(|x| Box::new(x) as _),
        read_fonts::tables::glyf::TAG => font.glyf().map(|x| Box::new(x) as _),
        read_fonts::tables::head::TAG => font.head().map(|x| Box::new(x) as _),
        read_fonts::tables::hhea::TAG => font.hhea().map(|x| Box::new(x) as _),
        read_fonts::tables::hmtx::TAG => font.hmtx().map(|x| Box::new(x) as _),
        read_fonts::tables::loca::TAG => font.loca(None).map(|x| Box::new(x) as _),
        read_fonts::tables::maxp::TAG => font.maxp().map(|x| Box::new(x) as _),
        read_fonts::tables::name::TAG => font.name().map(|x| Box::new(x) as _),
        read_fonts::tables::post::TAG => font.post().map(|x| Box::new(x) as _),
        _ => Err(ReadError::TableIsMissing(tag)),
    }
}

fn print_table(font: &FontRef, tag: Tag) {
    match get_some_table(font, tag) {
        Ok(table) => fancy_print_table(&table).unwrap(),
        Err(err) => println!("{tag}: Error '{err}'"),
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

fn fancy_print_table<'a>(table: &(dyn SomeTable<'a> + 'a)) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut locked = stdout.lock();
    let mut formatter = PrettyPrinter::new(&mut locked);
    formatter.print_root_table(table)
}

mod flags {
    use super::Query;
    use std::path::PathBuf;

    xflags::xflags! {
        /// Generate font table representations
        cmd args
            required input: PathBuf
            {
                optional -l, --list
                optional -q, --query query: Query
                optional -t, --tables include: String
                optional -x, --exclude exclude: String
            }

    }
}
