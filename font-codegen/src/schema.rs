//! Generating schema files from our internal types

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::parsing::{
    self, BitFlags, Field as RawField, FieldType, Item, Items, RawEnum as RawRawRnum, RawVariant,
    Record as RawRecord, Table as RawTable, TableFormat, TableReadArgs,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Type(String);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
enum SchemaItem {
    Table(Table),
    Record(Record),
    Flags(Flags),
    RawEnum(RawEnum),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Table {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    sfnt_tag: Option<String>,
    short_doc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    long_doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    doc_link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    input_args: Option<Vec<InputArgument>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    formats: Vec<FormatTable>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Record {
    name: String,
    short_doc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    input_args: Option<Vec<InputArgument>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InputArgument {
    name: String,
    #[serde(rename = "type")]
    type_: Type,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum OutputArgument {
    Field(String),
    Literal(String), //FIXME: what type here?
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FormatTable {
    format_type: Type,
    format: i64,
    table: Table,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Field {
    name: String,
    #[serde(rename = "type")]
    type_: Type,
    doc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    offset: Option<OffsetInfo>,
    /// Presence of this field indicates this is an array.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    count: Option<CountInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    available: Option<AvailableInfo>,
    hidden: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OffsetInfo {
    nullable: bool,
    target: OffsetTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum OffsetTarget {
    Table(OffsetTargetType),
    Array(OffsetTargetType),
    Map(OffsetTargetMap),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OffsetTargetType {
    target: Type,
    arguments: Vec<OutputArgument>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OffsetTargetMap {
    argument: InputArgument,
    target_map: BTreeMap<String, OffsetTargetType>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum CountInfo {
    All,
    Computed(ComputedCount),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum CountArg {
    Literal(u32),
    Field(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Flags {
    name: String,
    short_doc: String,
    // must be an integer
    #[serde(rename = "type")]
    type_: Type,
    values: Vec<NamedValue>,
}

//HMM: this is structurally identical to the above, it just has different semantics
#[derive(Clone, Debug, Deserialize, Serialize)]
struct RawEnum {
    name: String,
    short_doc: String,
    // must be an integer
    #[serde(rename = "type")]
    type_: Type,
    values: Vec<NamedValue>,
}

// shared between enums and flags
#[derive(Clone, Debug, Deserialize, Serialize)]
struct NamedValue {
    name: String,
    doc: String,
    value: u32,
}

impl From<parsing::CountArg> for CountArg {
    fn from(src: parsing::CountArg) -> Self {
        match src {
            parsing::CountArg::Field(ident) => CountArg::Field(ident.to_string()),
            parsing::CountArg::Literal(lit) => CountArg::Literal(lit.base10_parse().unwrap()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ComputedCount {
    /// named fields of the parent
    inputs: Vec<CountArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    transform: Option<parsing::CountTransform>,
}

impl Serialize for parsing::CountTransform {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for parsing::CountTransform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AvailableInfo {
    major: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    minor: Option<u32>,
}

pub(crate) fn generate(items: &Items) -> Result<String, syn::Error> {
    let mut done = HashSet::new();
    let mut out = Vec::new();
    // first we do groups
    for item in items.iter().filter_map(|item| match item {
        crate::parsing::Item::Format(group) => Some(group),
        _ => None,
    }) {
        done.insert(item.name.clone());
        let table = generate_table_group(item, items, &mut done);
        out.push(serde_yaml::to_string(&SchemaItem::Table(table)).unwrap());
    }

    out.extend(
        items
            .iter()
            .filter_map(|item| match item {
                Item::Table(table) if done.insert(table.raw_name().clone()) => {
                    Some(SchemaItem::Table(generate_table(table)))
                }

                Item::Record(record) => Some(SchemaItem::Record(generate_record(record))),
                Item::RawEnum(raw) => Some(SchemaItem::RawEnum(generate_raw_enum(raw))),
                Item::Flags(raw) => Some(SchemaItem::Flags(generate_flags(raw))),
                //Item::GenericGroup(_) => todo!(),
                //Item::Extern(_) => todo!(),
                _ => None,
            })
            .map(|x| serde_yaml::to_string(&x).unwrap()),
    );

    Ok(out.join("\n"))
    //}
}

fn generate_table_group(
    item: &TableFormat,
    items: &Items,
    done: &mut HashSet<syn::Ident>,
) -> Table {
    let format_type = Type(item.format.to_string());
    let sfnt_tag = None;
    let short_doc = doc_attrs_to_string(&item.attrs.docs);
    let formats = item
        .variants
        .iter()
        .map(|variant| {
            let Some(Item::Table(table)) = items.get(variant.type_name()) else {
            panic!("missing table '{}'", variant.type_name());
        };
            assert!(done.insert(table.raw_name().clone())); // should never already be visited
            let format = table
                .fields
                .iter()
                .find_map(|fld| {
                    fld.attrs
                        .format
                        .as_deref()
                        .map(|format| format.base10_parse::<i64>().unwrap())
                })
                .expect("missing format field");
            let table = generate_table(table);
            FormatTable {
                format_type: format_type.clone(),
                format,
                table,
            }
        })
        .collect();
    Table {
        name: item.name.to_string(),
        sfnt_tag,
        short_doc,
        formats,
        ..Default::default()
    }
}

fn generate_table(item: &RawTable) -> Table {
    let name = item.raw_name().to_string();
    let sfnt_tag = None;
    let short_doc = doc_attrs_to_string(&item.attrs.docs);
    let input_args = item.attrs.read_args.as_deref().map(generate_input_args);
    let version = item
        .fields
        .iter()
        .find_map(|fld| fld.attrs.version.is_some().then(|| fld.name.to_string()));
    let fields = item.fields.iter().map(generate_field).collect();
    Table {
        name,
        sfnt_tag,
        short_doc,
        input_args,
        version,
        fields,
        long_doc: None,
        doc_link: None,
        formats: Default::default(),
    }
}

fn generate_record(item: &RawRecord) -> Record {
    Record {
        name: item.name.to_string(),
        short_doc: doc_attrs_to_string(&item.attrs.docs),
        input_args: item.attrs.read_args.as_deref().map(generate_input_args),
        fields: item.fields.iter().map(generate_field).collect(),
    }
}

fn generate_input_args(args: &TableReadArgs) -> Vec<InputArgument> {
    args.args
        .iter()
        .map(|arg| InputArgument {
            name: arg.ident.to_string(),
            type_: Type(arg.typ.to_string()),
        })
        .collect()
}

fn generate_raw_enum(raw: &RawRawRnum) -> RawEnum {
    RawEnum {
        name: raw.name.to_string(),
        short_doc: doc_attrs_to_string(&raw.docs),
        type_: Type(raw.typ.to_string()),
        values: raw.variants.iter().map(generate_value).collect(),
    }
}

fn generate_flags(raw: &BitFlags) -> Flags {
    Flags {
        name: raw.name.to_string(),
        short_doc: doc_attrs_to_string(&raw.docs),
        type_: Type(raw.typ.to_string()),
        values: raw.variants.iter().map(generate_value).collect(),
    }
}

fn generate_value(from: &RawVariant) -> NamedValue {
    NamedValue {
        name: from.name.to_string(),
        doc: doc_attrs_to_string(&from.docs),
        value: from.value.base10_parse().unwrap(),
    }
}

fn generate_field(field: &RawField) -> Field {
    let name = field.name.to_string();
    let type_ = match &field.typ {
        FieldType::Offset { typ, .. } | FieldType::Scalar { typ } | FieldType::Struct { typ } => {
            Type(typ.to_string())
        }
        FieldType::Array { inner_typ } => Type(inner_typ.cooked_type_tokens().to_string()),
        FieldType::ComputedArray(arr) | FieldType::VarLenArray(arr) => {
            Type(arr.raw_inner_type().to_string())
        }
        FieldType::PendingResolution { .. } => panic!("resolved before now"),
    };

    let doc = doc_attrs_to_string(&field.attrs.docs);
    let offset = match &field.typ {
        FieldType::Offset { target, .. } => {
            let arguments = field
                .attrs
                .read_offset_args
                .as_deref()
                .map(|args| {
                    args.inputs
                        .iter()
                        .map(|inp| OutputArgument::Field(inp.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let target = match target {
                crate::parsing::OffsetTarget::Table(ident) => {
                    OffsetTarget::Table(OffsetTargetType {
                        target: Type(ident.to_string()),
                        arguments,
                    })
                }
                crate::parsing::OffsetTarget::Array(inner) => {
                    OffsetTarget::Array(OffsetTargetType {
                        target: Type(inner.cooked_type_tokens().to_string()),
                        arguments,
                    })
                }
            };

            Some(OffsetInfo {
                nullable: field.attrs.nullable.is_some(),
                target,
            })
        }
        _ => None,
    };
    let count = field.attrs.count.as_deref().map(|count| match count {
        parsing::Count::All(_) => CountInfo::All,
        parsing::Count::SingleArg(arg) => CountInfo::Computed(ComputedCount {
            inputs: vec![arg.clone().into()],
            transform: None,
        }),
        parsing::Count::Complicated { args, xform } => CountInfo::Computed(ComputedCount {
            inputs: args.iter().cloned().map(Into::into).collect(),
            transform: Some(*xform),
        }),
    });
    let available = field.attrs.available.as_deref().map(|avail| AvailableInfo {
        major: avail.major.base10_parse().unwrap(),
        minor: avail.minor.as_ref().map(|v| v.base10_parse().unwrap()),
    });
    let hidden = false;
    Field {
        name,
        type_,
        doc,
        offset,
        count,
        available,
        hidden,
    }
}

fn doc_attrs_to_string(docs: &[syn::Attribute]) -> String {
    let mut out = String::new();
    for doc in docs {
        let as_str = doc.tokens.to_string();
        let as_str = as_str.trim_matches(['=', ' ', '"'].as_slice());
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(as_str)
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_enum() {
        let an_enum = RawEnum {
            name: "Hello".into(),
            short_doc: "I am a useful type that performs many important tasks".into(),
            type_: Type("u32".into()),
            values: vec![NamedValue {
                name: "INFLAMMIBLE".into(),
                doc: "Ugh I always forget which is which".into(),
                value: 1,
            }],
        };

        let to_str = serde_yaml::to_string(&an_enum).unwrap();
        let _back_to_typeland: RawEnum = serde_yaml::from_str(&to_str).unwrap();
    }
}
