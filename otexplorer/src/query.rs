//! querying paths within fonts

use std::{borrow::Cow, fmt::Write, str::FromStr};

use font_types::Tag;
use read_fonts::{
    traversal::{Field, FieldType, ResolvedOffset, SomeTable},
    FontRef,
};

#[derive(Clone, Debug)]
pub struct Query {
    tag: Tag,
    elements: Vec<QueryElement>,
}

#[derive(Debug, Clone)]
pub enum QueryElement {
    Field(String),
    Index(u32),
}

pub fn print_query(font: &FontRef, query: &Query) -> Result<(), String> {
    let table = match super::get_some_table(font, query.tag) {
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
            super::fancy_print_table(&table).unwrap();
            Ok(())
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

fn print_field(field: FieldType) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut locked = stdout.lock();
    let mut formatter = super::PrettyPrinter::new(&mut locked);

    formatter.print_table_header()?;
    formatter.print_field(&field)?;
    formatter.print_newline()?;
    formatter.print_table_footer()
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

/// returns `true` if every byte in the query is present in the field, in the same order.
fn ascii_fuzzy_match(query: &str, field: &str) -> bool {
    let mut fld_pos = 0;
    //let mut hits = 0;
    for query_byte in query.bytes().map(|b| b.to_ascii_lowercase()) {
        match field.bytes().skip(fld_pos).position(|b| b == query_byte) {
            Some(pos) => fld_pos += pos + 1,
            None => return false,
        }
    }
    true
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
        FieldType::Array(arr) => format!("[{}]", arr.type_name()).into(),
        FieldType::Record(record) => record.type_name().to_string().into(),
        FieldType::ValueRecord(_) => "ValueRecord".into(),
        FieldType::ResolvedOffset(ResolvedOffset {
            target: Ok(table), ..
        }) => table.type_name().to_string().into(),
        FieldType::ResolvedOffset(_) | FieldType::BareOffset(_) => "Offset".into(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy() {
        assert!(!ascii_fuzzy_match("off", "lookup_flag"));
    }
}
