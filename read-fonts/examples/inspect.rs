//! Print the contents of font tables.
//!
//! This accepts command line arguments similar to what is present in ttx,
//! although it does not produce xml output.

use std::{borrow::Cow, collections::HashSet, fmt::Write, str::FromStr};

use font_types::Tag;
use read_fonts::{
    traversal::{Field, FieldType, OffsetType, ResolvedOffset, SomeArray, SomeTable},
    FontData, FontRef, ReadError, TableProvider,
};

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
        return print_query(&font, query).map_err(Error);
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
    hex_width(max_off)
}

fn hex_width(val: u32) -> usize {
    match val {
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

fn get_some_table<'a>(
    font: &FontRef<'a>,
    tag: Tag,
) -> Result<Box<dyn SomeTable<'a> + 'a>, ReadError> {
    match tag {
        read_fonts::tables::gpos::TAG => font.gpos().map(|x| Box::new(x) as _),
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

fn print_query(font: &FontRef, query: &Query) -> Result<(), String> {
    let table = match get_some_table(font, query.tag) {
        Ok(table) => table,
        Err(err) => return Err(err.to_string()),
    };

    match query.elements.split_first() {
        Some((QueryElement::Field(name), rest)) => {
            let field = get_field(&table, name)?;
            let mut used_path = vec![QueryElement::Field(field.name.to_string())];
            let target = find_query_recursive(field.typ, rest, &mut used_path)?;
            print_used_query(query, &used_path);
            println!("found {}", field_type_name(&target));
            println!();
            print_field(target).map_err(|e| format!("print failed: '{e}'"))
        }
        Some((QueryElement::Index(_), _)) => Err("tables cannot be indexed".into()),
        None => {
            fancy_print_table(&table).unwrap();
            Ok(())
        }
    }
}

fn print_used_query(query: &Query, used: &[QueryElement]) {
    let tag = query.tag;
    let mut q_string = format!("query {tag}");
    let mut m_string = format!("match {tag}");

    for (q_elem, m_elem) in query.elements.iter().zip(used) {
        match (q_elem, m_elem) {
            (QueryElement::Field(name1), QueryElement::Field(name2)) => {
                let padding = name1.len().max(name2.len());
                write!(&mut q_string, ".{name1:padding$}").unwrap();
                write!(&mut m_string, ".{name2:padding$}").unwrap();
            }
            (QueryElement::Index(idx), QueryElement::Index(_)) => {
                write!(&mut q_string, "[{idx}]").unwrap();
                write!(&mut m_string, "[{idx}]").unwrap();
            }
            _ => panic!("this should not happen"),
        }
    }

    println!("{q_string}");
    println!("{m_string}");
}

fn find_query_recursive<'a>(
    current: FieldType<'a>,
    query_path: &[QueryElement],
    used_path: &mut Vec<QueryElement>,
) -> Result<FieldType<'a>, String> {
    let (next, rest) = match query_path.split_first() {
        Some(thing) => thing,
        None => return Ok(current),
    };

    match next {
        QueryElement::Field(name) => {
            let field = match current {
                FieldType::ResolvedOffset(ResolvedOffset { target, .. }) => match target {
                    Ok(table) => get_field(&table, name),
                    Err(err) => Err(format!("Error reading offset for field '{name}': '{err}'")),
                },
                FieldType::Record(record) => get_field(&record, name),
                _ => Err(format!(
                    "No field '{name}' on type '{}'",
                    field_type_name(&current)
                )),
            }?;
            used_path.push(QueryElement::Field(field.name.to_string()));
            find_query_recursive(field.typ, rest, used_path)
        }
        QueryElement::Index(idx) => {
            let field = match current {
                FieldType::Array(arr) => arr
                    .get(*idx as usize)
                    .ok_or_else(|| format!("index {idx} out of bounds for array")),
                FieldType::OffsetArray(arr) => arr
                    .iter()
                    .nth(*idx as usize)
                    .ok_or_else(|| format!("index {idx} out of bounds for array")),
                _ => Err(format!(
                    "Index provided but field type '{}' is not indexable",
                    field_type_name(&current)
                )),
            }?;
            used_path.push(next.clone());
            find_query_recursive(field, rest, used_path)
        }
    }
}

fn get_field<'a>(table: &(dyn SomeTable<'a> + 'a), name: &str) -> Result<Field<'a>, String> {
    let mut result = None;
    for field in table.iter() {
        if ascii_fuzzy_match(name, field.name) {
            match result.take() {
                None => result = Some(field),
                Some(prev) => {
                    return Err(format!(
                        "Error: ambiguous query path '{name}' (matches '{}' and '{}')",
                        prev.name, field.name
                    ))
                }
            }
        }
    }

    result.ok_or_else(|| format!("{} contains no field '{name}'", table.type_name()))
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

    fn indented(
        &mut self,
        f: impl FnOnce(&mut PrettyPrinter) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn print_newline(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"\n")
    }

    fn print_root_table<'b>(&mut self, table: &(dyn SomeTable<'b> + 'b)) -> std::io::Result<()> {
        self.print_indent()?;
        self.writer.write_all(table.type_name().as_bytes())?;
        self.print_newline()?;
        self.indented(|this| this.print_fields(table))?;
        self.print_newline()
    }

    fn print_fields<'b>(&mut self, table: &(dyn SomeTable<'b> + 'b)) -> std::io::Result<()> {
        for (i, field) in table.iter().enumerate() {
            if i != 0 {
                self.print_newline()?;
            }
            self.print_indent()?;
            self.writer.write_all(field.name.as_bytes())?;
            self.writer.write_all(": ".as_bytes())?;
            self.print_field(&field.typ)?;
        }
        Ok(())
    }

    fn print_array<'b>(&mut self, array: &(dyn SomeArray<'b> + 'b)) -> std::io::Result<()> {
        self.writer
            .write_fmt(format_args!("[{}]\n", array.type_name()))?;
        self.indented(|this| {
            for (i, item) in array.iter().enumerate() {
                if i != 0 {
                    this.print_newline()?;
                }
                if is_scalar(&item) {
                    this.print_indent()?;
                }
                this.print_field(&item)?;
            }
            Ok(())
        })
    }

    fn print_offset(&mut self, offset: OffsetType) -> std::io::Result<()> {
        let offset = offset.to_u32();
        let hex_width = hex_width(offset);
        self.writer
            .write_fmt(format_args!("0x{offset:0hex_width$X}"))
    }

    fn print_field<'b>(&mut self, field: &FieldType<'b>) -> std::io::Result<()> {
        match &field {
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
            FieldType::GlyphId(val) => self.writer.write_fmt(format_args!("{}", val.to_u16()))?,
            FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => {
                self.print_offset(*offset)?;
                self.print_newline()?;
                match target {
                    Ok(table) => self.indented(|this| this.print_fields(table)),
                    Err(e) => self.writer.write_fmt(format_args!("Error: '{e}'")),
                }?;
            }
            FieldType::BareOffset(offset) => self.print_offset(*offset)?,
            FieldType::Record(record) => self.print_fields(record)?,
            FieldType::ValueRecord(record) if record.get_field(0).is_none() => {
                self.writer.write_all(b"Null")?
            }
            FieldType::ValueRecord(record) => self.indented(|this| {
                this.print_newline()?;
                this.print_fields(record)
            })?,
            FieldType::Array(array) => self.print_array(array)?,
            FieldType::OffsetArray(array) => {
                for (i, table) in array.iter().enumerate() {
                    if i != 0 {
                        self.writer.write_all(b",")?;
                    }
                    self.print_newline()?;
                    self.indented(|this| {
                        this.print_indent()?;
                        this.print_field(&table)
                    })?;
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

fn print_field(field: FieldType) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut locked = stdout.lock();
    let mut formatter = PrettyPrinter {
        depth: 0,
        writer: &mut locked,
        indent_size: 2,
    };

    formatter.print_field(&field)?;
    formatter.print_newline()
}

pub trait Print {
    fn print(&self, printer: &mut PrettyPrinter) -> std::io::Result<()>;
}

struct PrintTable<'a, 'b>(&'b (dyn SomeTable<'a> + 'a));

impl Print for PrintTable<'_, '_> {
    fn print(&self, printer: &mut PrettyPrinter) -> std::io::Result<()> {
        printer.print_root_table(self.0)?;
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

#[derive(Clone, Debug)]
pub struct Query {
    tag: Tag,
    elements: Vec<QueryElement>,
}

#[derive(Debug, Clone)]
enum QueryElement {
    Field(String),
    Index(u32),
}

impl FromStr for Query {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut components = s.split('.');
        let tag = match components.next() {
            None => Err("Query string should be non-empty".into()),
            Some(s) => s.parse::<Tag>().map_err(|s| s.to_string()),
        }?;

        let elements = components
            .map(|comp| match comp.chars().next() {
                Some('0'..='9') => comp
                    .parse::<u32>()
                    .map_err(|_| format!("invalid index '{comp}'"))
                    .map(QueryElement::Index),
                Some(_) => Ok(QueryElement::Field(comp.into())),
                None => Err("Empty query elements are not allowed".into()),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Query { tag, elements })
    }
}

fn field_type_name(field_type: &FieldType) -> Cow<'static, str> {
    match field_type {
        FieldType::I8(_) => "i8".into(),
        FieldType::U8(_) => "u8".into(),
        FieldType::I16(_) => "i16".into(),
        FieldType::U16(_) => "u16".into(),
        FieldType::I32(_) => "i32".into(),
        FieldType::U32(_) => "u32".into(),
        FieldType::U24(_) => "u24".into(),
        FieldType::Tag(_) => "Tag".into(),
        FieldType::FWord(_) => "FWord".into(),
        FieldType::UfWord(_) => "UfWord".into(),
        FieldType::MajorMinor(_) => "MajorMinor".into(),
        FieldType::Version16Dot16(_) => "Version16Dot16".into(),
        FieldType::F2Dot14(_) => "F2Dot14".into(),
        FieldType::Fixed(_) => "Fixed".into(),
        FieldType::LongDateTime(_) => "LongDateTime".into(),
        FieldType::GlyphId(_) => "GlyphId".into(),
        FieldType::Array(_) | FieldType::OffsetArray(_) => "Array".into(),
        FieldType::Record(record) => record.type_name().to_string().into(),
        FieldType::ValueRecord(_) => "ValueRecord".into(),
        FieldType::ResolvedOffset(ResolvedOffset {
            target: Ok(table), ..
        }) => table.type_name().to_string().into(),
        FieldType::ResolvedOffset(_) | FieldType::BareOffset(_) => "Offset".into(),
        FieldType::None => "None".into(),
    }
}

/// returns `true` if every byte in the query is present in the field, in the same order.
fn ascii_fuzzy_match(query: &str, field: &str) -> bool {
    let mut fld_pos = 0;
    //let mut hits = 0;
    for query_byte in query.bytes().map(|b| b.to_ascii_lowercase()) {
        match field.bytes().skip(fld_pos).position(|b| b == query_byte) {
            Some(pos) => fld_pos += pos,
            None => return false,
        }
    }
    true
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
