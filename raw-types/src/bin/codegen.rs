//! Helper to crudely rewrite types from the spec's format to our own.
//!
//! This reads a file containing table and record descriptions (from the spec)
//! and converts them to the form that we use, writing the result to stdout.
//!
//! Input should be in the format:
//!
//!
//! ```
//! Gpos1_0
//! uint16      majorVersion       Major version of the GPOS table, = 1
//! uint16      minorVersion       Minor version of the GPOS table, = 0
//! Offset16    scriptListOffset   Offset to ScriptList table, from beginning of GPOS table
//! Offset16    featureListOffset  Offset to FeatureList table, from beginning of GPOS table
//! Offset16    lookupListOffset   Offset to LookupList table, from beginning of GPOS table
//!
//! Gpos1_1
//! uint16      majorVersion            Major version of the GPOS table, = 1
//! uint16      minorVersion            Minor version of the GPOS table, = 1
//! Offset16    scriptListOffset        Offset to ScriptList table, from beginning of GPOS table
//! Offset16    featureListOffset       Offset to FeatureList table, from beginning of GPOS table
//! Offset16    lookupListOffset        Offset to LookupList table, from beginning of GPOS table
//! Offset32    featureVariationsOffset Offset to FeatureVariations table, from beginning of GPOS table (may be NULL)
//! ```
//!
//! - different records/tables are separated by newlines.
//! - the first line should be a single word, used as the name of the type
//! - other lines are just copy pasted
//!
//! *limitations:* this doesn't handle lifetimes, and doesn't generate annotations.
//! You will need to clean up the output.

macro_rules! exit_with_msg {
    ($disp:expr) => {{
        eprintln!("{}", $disp);
        std::process::exit(1);
    }};
}
fn main() {
    let in_path = std::env::args().nth(1).expect("expected path argument");
    let input = std::fs::read_to_string(in_path).expect("failed to read path");
    let mut lines = input.lines().map(str::trim);
    while let Some(item) = generate_one_item(&mut lines) {
        println!("{}", item);
    }
}

/// parse a single table or record.
///
/// Returns `Some` on success, `None` if there are no more items, and terminates
/// if something goes wrong.
fn generate_one_item<'a>(lines: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut lines = lines.skip_while(|line| line.is_empty());

    let name = match lines.next() {
        Some(line) if line.split_whitespace().nth(1).is_none() => line,
        Some(line) => exit_with_msg!(format!("expected table or record name, found '{}'", line)),
        _ => return None,
    };

    let field_text = lines.map_while(parse_field);
    let mut result = format!("{} {{\n", name);
    for line in field_text {
        result.push_str(&line);
        result.push('\n');
    }
    result.push_str("}\n");
    Some(result)
}

fn parse_field(line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    let mut iter = line.splitn(3, |c: char| c.is_ascii_whitespace());
    let (typ, ident, comment) = match (iter.next(), iter.next(), iter.next()) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => exit_with_msg!(format!(
            "line could not be parsed as type/name/comment: '{}'",
            line
        )),
    };
    let typ = normalize_type(typ);
    let ident = decamalize(ident);
    let comment = format_comment("    ", comment);
    Some(format!("{}\n    {}: {},", comment, ident, typ))
}

fn normalize_type(input: &str) -> &str {
    match input {
        "uint8" => "Uint8",
        "uint16" => "Uint16",
        "uint24" => "Uint24",
        "uint32" => "Uint32",
        "int8" => "Int8",
        "int16" => "Int16",
        "int32" => "Int32",
        "FWORD" => "FWord",
        "UFWORD" => "UfWord",
        "F2DOT14" => "F2Dot14",
        "LONGDATETIME" => "LongDateTime",
        "Tag" | "Fixed" | "Offset16" | "Offset32" | "Offset24" | "Version16Dot16" => input,
        other => exit_with_msg!(format!("unknown type '{}'", other)),
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

fn format_comment(whitespace: &str, input: &str) -> String {
    const LINE_LEN: usize = 72;

    let mut docs = String::new();
    //docs.push_str()

    let mut cur_len = docs.len();
    for token in input.split_inclusive(' ') {
        if docs.is_empty() || cur_len + token.len() > LINE_LEN {
            if !docs.is_empty() {
                docs.push('\n');
            }
            docs.push_str(whitespace);
            docs.push_str("/// ");
            cur_len = whitespace.len() + 4;
        }
        docs.push_str(token);
        cur_len += token.len();
    }
    docs
}
