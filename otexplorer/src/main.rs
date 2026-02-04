//! Print the contents of font tables.
//!
//! This accepts command line arguments similar to what is present in ttx,
//! although it does not produce xml output.

use std::{collections::HashSet, str::FromStr};

use font_types::Tag;
use read_fonts::{traversal::SomeTable, FileRef, FontRef, ReadError, TableProvider, TopLevelTable};

mod print;
mod query;

use print::PrettyPrinter;
use query::Query;

fn main() -> Result<(), Error> {
    let args = flags::Otexplorer::from_env_or_exit();
    let bytes = std::fs::read(&args.input).unwrap();
    let font = FileRef::new(&bytes)
        .unwrap()
        .fonts()
        .nth(args.index.unwrap_or(0) as usize)
        .unwrap()
        .unwrap();
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

    for record in font.table_directory().table_records() {
        println!(
            "{0} 0x{1:02$X} {3:8} 0x{4:08X} ",
            record.tag(),
            record.offset(),
            offset_pad,
            record.length(),
            record.checksum()
        );
    }
}

fn print_tables(font: &FontRef, filter: &TableFilter) {
    let mut printed = HashSet::new();
    for tag in font
        .table_directory()
        .table_records()
        .iter()
        .map(|rec| rec.tag())
        .filter(|tag| filter.should_print(*tag))
    {
        printed.insert(tag);
        print_table(font, tag)
    }

    if let TableFilter::Include(to_print) = filter {
        for unprinted in to_print.difference(&printed) {
            println!("Table {unprinted} not found");
        }
    }
}

fn get_offset_width(font: &FontRef) -> usize {
    // pick how much padding we use for offsets based on the max offset in directory
    let max_off = font
        .table_directory()
        .table_records()
        .iter()
        .map(|rec| rec.offset())
        .max()
        .unwrap_or_default();
    hex_width(max_off)
}

/// Return the number of hex digits required to represent a given number.
fn hex_width(val: u32) -> usize {
    match val {
        0..=0xffff => 4usize,
        0x10000..=0xffffff => 6,
        0x1000000.. => 8,
    }
}

/// Given a font and a tag, return the appropriate table as a [`dyn SomeTable`][SomeTable].
fn get_some_table<'a>(
    font: &FontRef<'a>,
    tag: Tag,
) -> Result<Box<dyn SomeTable<'a> + 'a>, ReadError> {
    use read_fonts::tables;
    match tag {
        tables::gpos::Gpos::TAG => font.gpos().map(|x| Box::new(x) as _),
        tables::gsub::Gsub::TAG => font.gsub().map(|x| Box::new(x) as _),
        tables::cmap::Cmap::TAG => font.cmap().map(|x| Box::new(x) as _),
        tables::fvar::Fvar::TAG => font.fvar().map(|x| Box::new(x) as _),
        tables::avar::Avar::TAG => font.avar().map(|x| Box::new(x) as _),
        tables::gdef::Gdef::TAG => font.gdef().map(|x| Box::new(x) as _),
        tables::glyf::Glyf::TAG => font.glyf().map(|x| Box::new(x) as _),
        tables::head::Head::TAG => font.head().map(|x| Box::new(x) as _),
        tables::hhea::Hhea::TAG => font.hhea().map(|x| Box::new(x) as _),
        tables::hmtx::Hmtx::TAG => font.hmtx().map(|x| Box::new(x) as _),
        tables::loca::Loca::TAG => font.loca(None).map(|x| Box::new(x) as _),
        tables::maxp::Maxp::TAG => font.maxp().map(|x| Box::new(x) as _),
        tables::name::Name::TAG => font.name().map(|x| Box::new(x) as _),
        tables::post::Post::TAG => font.post().map(|x| Box::new(x) as _),
        tables::varc::Varc::TAG => font.varc().map(|x| Box::new(x) as _),
        tables::colr::Colr::TAG => font.colr().map(|x| Box::new(x) as _),
        tables::stat::Stat::TAG => font.stat().map(|x| Box::new(x) as _),
        tables::vhea::Vhea::TAG => font.vhea().map(|x| Box::new(x) as _),
        tables::vmtx::Vmtx::TAG => font.vmtx().map(|x| Box::new(x) as _),
        tables::svg::Svg::TAG => font.svg().map(|x| Box::new(x) as _),
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
    fn from_args(args: &flags::Otexplorer) -> Result<Self, Error> {
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
        /// Explore the contents of binary font files
        ///
        /// otexplorer can print a textual representation of the tables in
        /// a font file, or some subset of those tables.
        ///
        /// In addition to this, the -q option allows the user to provide a query,
        /// which can be used to print some particular subtable or value.
        ///
        /// Queries begin with a table tag, and then zero or more path elements,
        /// separated by a period ('.'). Path elements can be either the names
        /// of fields, or indexes into an array.
        ///
        /// For instance, to print the first subtable of the second lookup in
        /// the GPOS table, you can do,
        ///
        /// otexplorer $myfont -q GPOS.lookup_list.lookup_offsets.1.subtable_offsets.0
        ///
        /// Queries are case-insensitive, and fuzzily matched. The following
        /// two examples produce identical results to the example above:
        ///
        /// otexplorer $myfont -q GPOS.lookupListOffset.lookupOffsets.1.subtableOffsets.0
        /// otexplorer $myfont -q GPOS.look.off.1.off.0

        cmd otexplorer {
            required input: PathBuf
            optional -i, --index index: u32
            optional -l, --list
            optional -q, --query query: Query
            optional -t, --tables include: String
            optional -x, --exclude exclude: String
        }
    }
}
