//! Helper to crudely rewrite types from the spec's format to our own.
//!
//! This reads a file containing table and record descriptions (from the spec)
//! and converts them to the form that we use, writing the result to stdout.
//!
//! For more information about how this works, see the README

use std::{fmt::Write, ops::Deref};

macro_rules! exit_with_msg {
    ($disp:expr, $line:expr) => {{
        eprintln!("ERROR: {}", $disp);
        eprintln!("Line {}: '{}'", 1 + $line.number, $line.text);
        std::process::exit(1);
    }};
}

/// a wrapper around a line, so we can report errors with line numbers
struct Line<'a> {
    text: &'a str,
    number: usize,
}

impl Deref for Line<'_> {
    type Target = str;
    fn deref(&self) -> &<Self as Deref>::Target {
        self.text
    }
}

fn main() {
    let in_path = std::env::args().nth(1).expect("expected path argument");
    let input = std::fs::read_to_string(in_path).expect("failed to read path");
    let mut lines = input
        .lines()
        .enumerate()
        .map(|(number, text)| Line {
            text: text.trim(),
            number,
        })
        .filter(|l| !l.starts_with('#'));

    while let Some(item) = generate_one_item(&mut lines) {
        println!("{item}\n")
    }
}

/// parse a single enum, table or record.
///
/// Returns `Some` on success, `None` if there are no more items, and terminates
/// if something goes wrong.
fn generate_one_item<'a>(lines: impl Iterator<Item = Line<'a>>) -> Option<String> {
    let mut lines = lines.skip_while(|line| line.is_empty());
    let mut comments = Vec::new();
    let decl = loop {
        match lines.next() {
            Some(line) if line.starts_with("///") => comments.push(line.text),
            Some(line) if line.starts_with('@') => break Decl::parse(line).unwrap(),
            Some(line) => exit_with_msg!("expected table or record name", line),
            None => return None,
        }
    };

    let item = match decl.kind {
        DeclKind::RawEnum => generate_one_enum(decl, lines),
        DeclKind::Table | DeclKind::Record => generate_one_table(decl, lines),
        DeclKind::Flags => generate_one_flags(decl, lines),
    }?;
    let mut comments = comments.join("\n");
    if comments.is_empty() {
        Some(item)
    } else {
        comments.push('\n');
        comments.push_str(&item);
        Some(comments)
    }
}

/// Generate a single table or record (they're currently the same)
fn generate_one_table<'a>(decl: Decl, lines: impl Iterator<Item = Line<'a>>) -> Option<String> {
    let fields = lines.map_while(parse_field).collect::<Vec<_>>();
    let mut result = String::new();
    writeln!(&mut result, "{} {} {{", decl.kind, decl.name).unwrap();
    for line in &fields {
        writeln!(&mut result, "{line}").unwrap();
    }
    result.push('}');
    Some(result)
}

fn generate_one_enum<'a>(decl: Decl, lines: impl Iterator<Item = Line<'a>>) -> Option<String> {
    let fields = lines.map_while(parse_field).collect::<Vec<_>>();
    let mut result = String::new();
    writeln!(
        &mut result,
        "#[repr({})]\nenum {} {{",
        decl.annotation, decl.name
    )
    .unwrap();
    for line in &fields {
        writeln!(&mut result, "    {} = {},", line.name, line.typ).unwrap();
    }
    result.push('}');
    Some(result)
}

fn generate_one_flags<'a>(decl: Decl, lines: impl Iterator<Item = Line<'a>>) -> Option<String> {
    let fields = lines.map_while(parse_field).collect::<Vec<_>>();
    let mut result = String::new();
    writeln!(
        &mut result,
        "#[flags({})]\n{} {{",
        decl.annotation, decl.name
    )
    .unwrap();
    for line in &fields {
        format_comment(&mut result, "    ", line.comment).unwrap();
        writeln!(&mut result, "    {} = {},", line.name, line.typ).unwrap();
    }
    result.push('}');
    Some(result)
}

enum DeclKind {
    Table,
    Record,
    RawEnum,
    Flags,
}

impl std::fmt::Display for DeclKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DeclKind::Table => write!(f, "table"),
            DeclKind::Record => write!(f, "record"),
            DeclKind::RawEnum => write!(f, "enum"),
            DeclKind::Flags => write!(f, "flags"),
        }
    }
}

struct Decl<'a> {
    kind: DeclKind,
    annotation: &'a str,
    name: &'a str,
}

impl<'a> Decl<'a> {
    fn parse(line: Line<'a>) -> Option<Self> {
        let mut decl = line.text.split_whitespace();
        let mut annotation = "";
        let kind = match decl.next()? {
            "@table" => DeclKind::Table,
            "@record" => DeclKind::Record,
            x if x.starts_with("@enum(") || x.starts_with("@flags(") => {
                let repr = x.split_once('(').unwrap().1.trim_end_matches(')');
                if !["u8", "u16"].contains(&repr) {
                    exit_with_msg!(format!("unexpected enum/flag repr '{repr}'"), line);
                }
                annotation = repr;
                if x.starts_with("@enum") {
                    DeclKind::RawEnum
                } else {
                    DeclKind::Flags
                }
            }
            "@enum" | "@flags" => exit_with_msg!(
                "@enum/@flags requires explicit repr like: '@flags(u16)'",
                line
            ),
            other => exit_with_msg!(format!("unknown item kind '{other}'"), line),
        };
        let name = decl
            .next()
            .unwrap_or_else(|| exit_with_msg!("missing name", line));

        Some(Decl {
            kind,
            annotation,
            name,
        })
    }
}

struct Field<'a> {
    name: &'a str,
    maybe_count: Option<String>,
    typ: &'a str,
    comment: &'a str,
}

impl std::fmt::Display for Field<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        format_comment(f, "    ", self.comment)?;
        if self.name.contains("reserved") {
            writeln!(f, "    #[hidden]")?;
        }
        if let Some(count) = &self.maybe_count {
            writeln!(f, "    #[count(${count})]")?;
            write!(f, "    {}: [{}],", decamalize(self.name), self.typ)?;
        } else {
            write!(f, "    {}: {},", decamalize(self.name), self.typ)?;
        }
        Ok(())
    }
}

fn parse_field(line: Line) -> Option<Field> {
    if line.is_empty() {
        return None;
    }
    let mut iter = line.text.splitn(3, '\t');
    let (typ, ident, comment) = match (iter.next(), iter.next(), iter.next()) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        (Some(a), Some(b), None) => (a, b, ""),
        _ => exit_with_msg!("line could not be parsed as type/name/comment", line),
    };
    let typ = normalize_type(typ);
    let (name, maybe_count) = split_ident(ident);
    //let name = decamalize(name);
    let maybe_count = maybe_count.map(decamalize);
    Some(Field {
        name,
        maybe_count,
        typ,
        comment,
    })
}

/// takes an ident and splits it into the name and an optional count (if the item
/// is an array)
fn split_ident(input: &str) -> (&str, Option<&str>) {
    match input.split_once('[') {
        Some((front, back)) => (front, Some(back.trim_end_matches(']'))),
        None => (input, None),
    }
}

fn normalize_type(input: &str) -> &str {
    match input {
        "uint8" => "BigEndian<u8>",
        "uint16" => "BigEndian<u16>",
        "uint24" => "BigEndian<Uint24>",
        "int24" => "BigEndian<Int24>",
        "uint32" => "BigEndian<u32>",
        "int8" => "BigEndian<i8>",
        "int16" => "BigEndian<i16>",
        "int32" => "BigEndian<i32>",
        "FWORD" => "BigEndian<FWord>",
        "UFWORD" => "BigEndian<UfWord>",
        "F2DOT14" => "BigEndian<F2Dot14>",
        "LONGDATETIME" => "BigEndian<LongDateTime>",
        "Version16Dot16" => "BigEndian<Version16Dot16>",
        "Fixed" => "BigEndian<Fixed>",
        "Tag" => "BigEndian<Tag>",
        "Offset16" => "BigEndian<Offset16>",
        "Offset24" => "BigEndian<Offset24>",
        "Offset32" => "BigEndian<Offset32>",
        other => other,
    }
}

fn decamalize(input: &str) -> String {
    //taken from serde: https://github.com/serde-rs/serde/blob/7e19ae8c9486a3bbbe51f1befb05edee94c454f9/serde_derive/src/internals/case.rs#L69-L76
    let mut snake = String::new();
    for (i, ch) in input.char_indices() {
        if i > 0 && ch.is_uppercase() {
            snake.push('_');
        }
        snake.push(ch.to_ascii_lowercase());
    }
    snake
}

fn format_comment(f: &mut dyn std::fmt::Write, whitespace: &str, input: &str) -> std::fmt::Result {
    const LINE_LEN: usize = 72;

    let mut cur_len = 0;

    for token in input.split_inclusive(' ') {
        if cur_len == 0 || cur_len + token.len() > LINE_LEN {
            if cur_len > 0 {
                writeln!(f)?;
            }
            write!(f, "{whitespace}/// ")?;
            cur_len = whitespace.len() + 4;
        }
        write!(f, "{token}")?;
        cur_len += token.len();
    }
    if cur_len > 0 {
        writeln!(f)?;
    }
    Ok(())
}
