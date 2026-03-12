//! subsetter input parsing util functions
use std::collections::BTreeMap;

use write_fonts::{
    read::collections::{int_set::Domain, IntSet},
    types::{GlyphId, NameId, Tag},
};

use crate::SubsetError;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InstancingSpec {
    pub pin_all_axes_to_default: bool,
    pub axes: BTreeMap<Tag, AxisSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AxisSpec {
    PinToDefault,
    Range { min: f32, def: f32, max: f32 },
}

pub fn populate_gids(gid_str: &str) -> Result<IntSet<GlyphId>, SubsetError> {
    if gid_str.trim() == "*" {
        return Ok(IntSet::<GlyphId>::all());
    }

    let mut result = IntSet::empty();
    if gid_str.is_empty() {
        return Ok(result);
    }
    for gid in gid_str.split(',') {
        if let Some((start, end)) = gid.split_once('-') {
            let start: u32 = start
                .parse::<u32>()
                .map_err(|_| SubsetError::InvalidGid(start.to_owned()))?;
            let end: u32 = end
                .parse::<u32>()
                .map_err(|_| SubsetError::InvalidGid(end.to_owned()))?;
            if start > end {
                return Err(SubsetError::InvalidGidRange { start, end });
            }
            result.extend((start..=end).map(GlyphId::new));
        } else {
            let glyph_id: u32 = gid
                .parse::<u32>()
                .map_err(|_| SubsetError::InvalidGid(gid.to_owned()))?;
            result.insert(GlyphId::new(glyph_id));
        }
    }
    Ok(result)
}

/// parse input unicodes string, which is a comma/whitespace-separated list of Unicode codepoints or ranges as hex numbers,
/// optionally prefixed with 'U+', 'u', etc. For example: --unicodes=41-5a,61-7a adds ASCII letters, so does the more verbose --unicodes=U+0041-005A,U+0061-007A.
/// The special strings '*' will choose all Unicode characters mapped by the font.
pub fn parse_unicodes(unicode_str: &str) -> Result<IntSet<u32>, SubsetError> {
    if unicode_str.trim() == "*" {
        return Ok(IntSet::<u32>::all());
    }
    let mut result = IntSet::empty();
    if unicode_str.is_empty() {
        return Ok(result);
    }
    let re = regex::Regex::new(r"[><\+,;&#}{\\xXuUnNiI\n\t\v\f\r]").unwrap();
    let s = re.replace_all(unicode_str, " ");
    for cp in s.split_whitespace() {
        if let Some((start, end)) = cp.split_once('-') {
            let start: u32 = u32::from_str_radix(start, 16)
                .map_err(|_| SubsetError::InvalidUnicode(start.to_owned()))?;
            let end: u32 = u32::from_str_radix(end, 16)
                .map_err(|_| SubsetError::InvalidUnicode(end.to_owned()))?;
            if start > end {
                return Err(SubsetError::InvalidUnicodeRange { start, end });
            }
            result.extend(start..=end);
        } else {
            let unicode: u32 = u32::from_str_radix(cp, 16)
                .map_err(|_| SubsetError::InvalidUnicode(cp.to_owned()))?;
            result.insert(unicode);
        }
    }
    Ok(result)
}

/// Parse a comma or whitespace list of things
fn parse_list<T: Domain>(
    input_str: &str,
    parse_one: fn(&str) -> Result<T, SubsetError>,
) -> Result<IntSet<T>, SubsetError> {
    if input_str.trim() == "*" {
        return Ok(IntSet::all());
    }
    input_str
        .split(&[',', ' '])
        .filter(|raw| !raw.is_empty())
        .map(parse_one)
        .collect()
}

//parse input tag list string, which is a comma/whitespace-separated list of tags(layout script or feature or table name)
pub fn parse_tag_list(input_str: &str) -> Result<IntSet<Tag>, SubsetError> {
    parse_list(input_str, |raw| {
        Tag::new_checked(raw.as_bytes()).map_err(|_| SubsetError::InvalidTag(raw.to_owned()))
    })
}

//parse input name_IDs string, which is a comma/whitespace-separated list of nameIDs that will be retained
pub fn parse_name_ids(input_str: &str) -> Result<IntSet<NameId>, SubsetError> {
    parse_list(input_str, |raw| {
        raw.parse::<u16>()
            .map(NameId::from)
            .map_err(|_| SubsetError::InvalidId(raw.to_owned()))
    })
}

//parse input name_languages string, which is a comma/whitespace-separated list of langIDs that will be retained
pub fn parse_name_languages(input_str: &str) -> Result<IntSet<u16>, SubsetError> {
    parse_list(input_str, |raw| {
        raw.parse::<u16>()
            .map_err(|_| SubsetError::InvalidId(raw.to_owned()))
    })
}

/// Parse a HarfBuzz-style instancing spec string, returning the parsed axis settings.
pub fn parse_instancing_spec(input_str: &str) -> Result<InstancingSpec, SubsetError> {
    let mut spec = InstancingSpec::default();
    if input_str.trim().is_empty() {
        return Ok(spec);
    }

    for part in input_str
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|raw| !raw.is_empty())
    {
        let (raw_axis, raw_value) = part
            .split_once('=')
            .ok_or_else(|| SubsetError::InvalidId(part.to_owned()))?;

        if raw_axis == "*" {
            if raw_value != "drop" {
                return Err(SubsetError::InvalidId(raw_value.to_owned()));
            }
            spec.pin_all_axes_to_default = true;
            continue;
        }

        let axis_tag = parse_axis_tag(raw_axis)?;
        if raw_value == "drop" {
            spec.axes.insert(axis_tag, AxisSpec::PinToDefault);
            continue;
        }

        let (min, def, max) = parse_axis_range_from_string(raw_value)?;
        spec.axes
            .insert(axis_tag, AxisSpec::Range { min, def, max });
    }

    Ok(spec)
}

fn parse_axis_tag(raw: &str) -> Result<Tag, SubsetError> {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 4 {
        return Err(SubsetError::InvalidTag(raw.to_owned()));
    }

    let mut padded = [b' '; 4];
    padded[..bytes.len()].copy_from_slice(bytes);
    Ok(Tag::new(&padded))
}

fn parse_axis_range_from_string(raw: &str) -> Result<(f32, f32, f32), SubsetError> {
    if raw == "drop" {
        return Ok((f32::NAN, f32::NAN, f32::NAN));
    }

    let parts: Vec<&str> = raw.split(':').collect();
    match parts.len() {
        1 => {
            let value = parse_axis_value(parts[0])?;
            Ok((value, value, value))
        }
        2 => {
            let min = parse_axis_value_or_nan(parts[0])?;
            let max = parse_axis_value_or_nan(parts[1])?;
            Ok((min, f32::NAN, max))
        }
        3 => {
            let min = parse_axis_value_or_nan(parts[0])?;
            let def = parse_axis_value_or_nan(parts[1])?;
            let max = parse_axis_value_or_nan(parts[2])?;
            Ok((min, def, max))
        }
        _ => Err(SubsetError::InvalidId(raw.to_owned())),
    }
}

fn parse_axis_value(raw: &str) -> Result<f32, SubsetError> {
    raw.parse::<f32>()
        .map_err(|_| SubsetError::InvalidId(raw.to_owned()))
}

fn parse_axis_value_or_nan(raw: &str) -> Result<f32, SubsetError> {
    if raw.is_empty() {
        return Ok(f32::NAN);
    }

    parse_axis_value(raw)
}

#[test]
fn test_populate_gids() {
    let input = "1,5,7";
    let output = populate_gids(input).unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(GlyphId::new(1)));
    assert!(output.contains(GlyphId::new(5)));
    assert!(output.contains(GlyphId::new(7)));

    let output = populate_gids("*").unwrap();
    assert!(output.contains(GlyphId::new(1)));
    assert!(output.contains(GlyphId::new(0)));
    assert!(output.contains(GlyphId::new(7)));
}

#[test]
fn test_parse_unicodes() {
    let output = parse_unicodes("61 62,63").unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(97_u32));
    assert!(output.contains(98_u32));
    assert!(output.contains(99_u32));

    let output = parse_unicodes("u+61,U+62,x63").unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(97_u32));
    assert!(output.contains(98_u32));
    assert!(output.contains(99_u32));

    let output = parse_unicodes("u+61,U+65-67").unwrap();
    assert_eq!(output.len(), 4);
    assert!(output.contains(97_u32));
    assert!(output.contains(101_u32));
    assert!(output.contains(102_u32));
    assert!(output.contains(103_u32));
}

#[test]
fn test_parse_drop_tables() {
    let input = "cmap,GSUB OS/2 CFF";
    let output = parse_tag_list(input).unwrap();
    assert_eq!(output.len(), 4);
    assert!(output.contains(Tag::new(b"cmap")));
    assert!(output.contains(Tag::new(b"GSUB")));
    assert!(output.contains(Tag::new(b"OS/2")));
    assert!(output.contains(Tag::new(b"CFF ")));

    let input = "";
    let output = parse_tag_list(input).unwrap();
    assert!(output.is_empty());
}

#[test]
fn test_parse_name_ids() {
    let input = "7,8,9";
    let output = parse_name_ids(input).unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(NameId::new(7)));
    assert!(output.contains(NameId::new(8)));
    assert!(output.contains(NameId::new(9)));

    let input = "";
    let output = parse_name_ids(input).unwrap();
    assert!(output.is_empty());

    let output = parse_name_ids("7,8 9").unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(NameId::new(7)));
    assert!(output.contains(NameId::new(8)));
    assert!(output.contains(NameId::new(9)));

    let output = parse_name_ids("*").unwrap();
    assert!(output.contains(NameId::new(7)));
    assert!(output.contains(NameId::new(8)));
    assert!(output.contains(NameId::new(9)));
}

#[test]
fn test_parse_name_languages() {
    let input = "1033, ";
    let output = parse_name_languages(input).unwrap();
    assert_eq!(output.len(), 1);
    assert!(output.contains(0x409));

    let input = "";
    let output = parse_name_languages(input).unwrap();
    assert!(output.is_empty());

    let input = "*";
    let output = parse_name_languages(input).unwrap();
    assert!(output.contains(1));

    let output = parse_name_languages("1,2 5").unwrap();
    assert_eq!(output.len(), 3);
    assert!(output.contains(1));
    assert!(output.contains(2));
    assert!(output.contains(5));
}
