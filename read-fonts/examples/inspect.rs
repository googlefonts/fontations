//! Print the contents of font tables.
//!
//! This accepts command line arguments similar to what is present in ttx,
//! although it does not produce xml output.

use std::{collections::HashSet, str::FromStr};

use font_types::Tag;
use read_fonts::{
    traversal::{Field, FieldType, OffsetType, ResolvedOffset, SomeArray, SomeTable},
    FontData, FontRef, TableProvider,
};

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

fn get_offset_width(font: &FontRef) -> usize {
    // pick how much padding we use for offsets based on the max offset in directory
    let max_off = font
        .table_directory
        .table_records()
        .iter()
        .map(|rec| rec.offset().to_u32())
        .max()
        .unwrap_or_default();
    match max_off {
        0..=0xffff => 4usize,
        0x10000..=0xffff_ff => 6,
        0x1000000.. => 8,
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
        read_fonts::tables::gpos::TAG => fancy_print_table(&font.gpos().unwrap()).unwrap(),
        read_fonts::tables::cmap::TAG => fancy_print_table(&font.cmap().unwrap()).unwrap(),
        read_fonts::tables::gdef::TAG => fancy_print_table(&font.gdef().unwrap()).unwrap(),
        read_fonts::tables::glyf::TAG => fancy_print_table(&font.glyf().unwrap()).unwrap(),
        read_fonts::tables::head::TAG => fancy_print_table(&font.head().unwrap()).unwrap(),
        read_fonts::tables::hhea::TAG => fancy_print_table(&font.hhea().unwrap()).unwrap(),
        read_fonts::tables::hmtx::TAG => fancy_print_table(&font.hmtx().unwrap()).unwrap(),
        read_fonts::tables::loca::TAG => fancy_print_table(&font.loca(None).unwrap()).unwrap(),
        read_fonts::tables::maxp::TAG => fancy_print_table(&font.maxp().unwrap()).unwrap(),
        read_fonts::tables::name::TAG => fancy_print_table(&font.name().unwrap()).unwrap(),
        read_fonts::tables::post::TAG => fancy_print_table(&font.post().unwrap()).unwrap(),
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

pub struct PrettyPrinter<'a> {
    depth: usize,
    indent_size: usize,
    writer: &'a mut (dyn std::io::Write + 'a),
}

impl<'a> PrettyPrinter<'a> {
    fn print_indent(&mut self) -> std::io::Result<()> {
        static MANY_SPACES: [u8; 200] = [0x20; 200];
        let indent_len = (self.depth * self.indent_size).min(MANY_SPACES.len());
        self.writer.write_all(&MANY_SPACES[..indent_len])
    }

    fn write_newline(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"\n")
    }

    fn print_table<'b>(
        &mut self,
        offset: Option<OffsetType>,
        table: &(dyn SomeTable<'b> + 'b),
    ) -> std::io::Result<()> {
        if let Some(offset) = offset {
            self.writer
                .write_fmt(format_args!("{:04X} ", offset.to_u32()))?;
        }
        self.writer.write_all(table.type_name().as_bytes())?;
        self.depth += 1;
        for field in table.iter() {
            self.write_newline()?;
            self.add_field(&field)?;
        }
        self.depth = self.depth.saturating_sub(1);
        Ok(())
    }

    fn print_array<'b>(&mut self, array: &(dyn SomeArray<'b> + 'b)) -> std::io::Result<()> {
        self.writer.write_fmt(format_args!("[TypeName]\n"))?;
        self.depth += 1;
        for (i, item) in array.iter().enumerate() {
            if i != 0 {
                self.write_newline()?;
            }
            if is_scalar(&item) {
                self.print_indent()?;
            }
            match item {
                FieldType::I8(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::U8(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::I16(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::U16(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::I32(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::U32(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::U24(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::Tag(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::FWord(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::UfWord(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::MajorMinor(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::Version16Dot16(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::F2Dot14(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::Fixed(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::LongDateTime(val) => self.writer.write_fmt(format_args!("{val:?},"))?,
                FieldType::GlyphId(val) => self.writer.write_fmt(format_args!("{val},"))?,
                FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => match target {
                    Ok(table) => self.print_table(offset.into(), &table),
                    Err(e) => self.writer.write_fmt(format_args!("Error: '{e}'")),
                }?,
                FieldType::BareOffset(offset) => self
                    .writer
                    .write_fmt(format_args!("{:04X}", offset.to_u32()))?,
                FieldType::Record(record) => {
                    let record = &record as &dyn SomeTable;
                    for (i, field) in record.iter().enumerate() {
                        if i != 0 {
                            self.write_newline()?;
                        }
                        self.add_field(&field)?;
                    }
                }
                FieldType::ValueRecord(record) => {
                    let record = &record as &dyn SomeTable;
                    for (i, field) in record.iter().enumerate() {
                        if i != 0 {
                            self.write_newline()?;
                        }
                        self.add_field(&field)?;
                    }
                }
                FieldType::Array(_) | FieldType::OffsetArray(_) => {
                    unreachable!("there are no nested arrays")
                }
                //FieldType::OffsetArray(_) => todo!(),
                FieldType::None => self.writer.write_all(b"None,")?,
            }
        }
        self.depth -= 1;
        Ok(())
    }

    fn add_field<'b>(&mut self, field: &Field<'b>) -> std::io::Result<()> {
        self.print_indent()?;
        self.writer.write_all(field.name.as_bytes())?;
        self.writer.write_all(": ".as_bytes())?;
        match &field.typ {
            FieldType::I8(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U8(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::I16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::I32(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U32(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U24(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Tag(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::FWord(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::UfWord(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::MajorMinor(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Version16Dot16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::F2Dot14(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Fixed(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::LongDateTime(val) => self.writer.write_fmt(format_args!("{val:?}"))?,
            FieldType::GlyphId(val) => self.writer.write_fmt(format_args!("{val}"))?,
            //FieldType::ResolvedOffset(ResolvedOffset { target, .. }) => match target {

            //}
            FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => match target {
                Ok(table) => self.print_table((*offset).into(), table),
                Err(e) => self.writer.write_fmt(format_args!("Error: '{e}'")),
            }?,
            FieldType::BareOffset(offset) => self
                .writer
                .write_fmt(format_args!("{:04X}", offset.to_u32()))?,
            FieldType::Record(_) => (),
            FieldType::ValueRecord(record) if record.get_field(0).is_none() => {
                self.writer.write_all(b"Null")?
            }
            FieldType::ValueRecord(record) => self.print_table(None, record)?,
            FieldType::Array(array) => self.print_array(array)?,
            FieldType::OffsetArray(array) => {
                for table in array.iter() {
                    self.write_newline()?;
                    match table {
                        FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => {
                            self.print_indent()?;
                            match target {
                                Ok(table) => self.print_table(offset.into(), &table),
                                Err(e) => self.writer.write_fmt(format_args!("Error: '{e}'")),
                            }?
                        }
                        //FieldType::
                        //FieldType::ResolvedOffset(Ok(table)) => {
                        //self.print_indent()?;
                        //self.print_table(&table)?;
                        //}
                        //FieldType::ResolvedOffset(Err(e)) => {
                        //self.print_indent()?;
                        //self.writer.write_fmt(format_args!("Error: '{e}'"))?;
                        //}
                        FieldType::BareOffset(off) => {
                            self.print_indent()?;
                            self.writer
                                .write_fmt(format_args!("{:04X}", off.to_u32()))?;
                        }
                        FieldType::None => {
                            self.print_indent()?;
                            self.writer.write_all(b"None")?;
                        }
                        _ => unreachable!(
                            "this only contains offsets: {} {:?}",
                            field.name, field.typ
                        ),
                    }
                }
            }
            FieldType::None => self.writer.write_all(b"None")?,
        }
        Ok(())
    }
}

fn is_scalar(field_type: &FieldType) -> bool {
    matches!(
        field_type,
        FieldType::I8(_)
            | FieldType::U8(_)
            | FieldType::I16(_)
            | FieldType::U16(_)
            | FieldType::I32(_)
            | FieldType::U32(_)
            | FieldType::U24(_)
            | FieldType::Tag(_)
            | FieldType::FWord(_)
            | FieldType::UfWord(_)
            | FieldType::MajorMinor(_)
            | FieldType::Version16Dot16(_)
            | FieldType::F2Dot14(_)
            | FieldType::Fixed(_)
            | FieldType::LongDateTime(_)
            | FieldType::GlyphId(_)
    )
    //FieldType::ResolvedOffset(_) => todo!(),
    //FieldType::Record(_) => todo!(),
    //FieldType::ValueRecord(_) => todo!(),
    //FieldType::Array(_) => todo!(),
    //FieldType::OffsetArray(_) => todo!(),
    //FieldType::Unimplemented => todo!(),
    //FieldType::None => todo!(),
}

fn fancy_print_table<'a>(table: &(dyn SomeTable<'a> + 'a)) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut locked = stdout.lock();
    let mut formatter = PrettyPrinter {
        depth: 0,
        writer: &mut locked,
        indent_size: 2,
    };

    PrintTable(table).print(&mut formatter)
}

pub trait Print {
    fn print(&self, printer: &mut PrettyPrinter) -> std::io::Result<()>;
}

struct PrintTable<'a, 'b>(&'b (dyn SomeTable<'a> + 'a));

impl Print for PrintTable<'_, '_> {
    fn print(&self, printer: &mut PrettyPrinter) -> std::io::Result<()> {
        printer.print_table(None, self.0)?;
        Ok(())
    }
}

/* I want something like,
GDEF: Gdef
  version: 1.0,
  glyph_class_def_offset: ClassDefFormat2
    class_format: 2,
    class_range_count: 8,
    class_range_records: [ClassRangeRecord]
    + start_glyph_id: g261, // 0
    | end_glyph_id: g286,
    | class: 1,
    + start_glyph_id: g288, // 1
    | end_glyph_id: g297,
    | class: 1,
    + start_glyph_id: g298,
    | end_glyph_id: g308,
    | class: 3,
    + start_glyph_id: g321,
    | end_glyph_id: g321,
    | class: 1,
    + start_glyph_id: g322,
    | end_glyph_id: g322,
    | class: 3,
    + start_glyph_id: g340,
    | end_glyph_id: g340,
    | class: 3,
    + start_glyph_id: g341,
    | end_glyph_id: g350,
    | class: 1,
    + start_glyph_id: g354,
    | end_glyph_id: g354,
    | class: 3,
  attach_list_offset: None,
  lig_caret_list_offset: None,
  mark_attach_class_def_offset: None,
  mark_glyph_sets_def_offset: None,
  item_var_store_offset: None,
*/

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
