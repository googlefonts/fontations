//! subsetter input parsing util functions
use write_fonts::read::collections::{int_set::Domain, IntSet};
use write_fonts::types::{GlyphId, NameId, Tag};

use crate::SubsetError;

pub fn populate_gids(gid_str: &str) -> Result<IntSet<GlyphId>, SubsetError> {
    if gid_str.trim() == "*" {
        return Ok(IntSet::<GlyphId>::all());
    }

    let mut result = IntSet::empty();
    if gid_str.is_empty() {
        return Ok(result);
    }
    for gid in gid_str
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
    {
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

/// Resolve comma/whitespace-separated glyph names using post or CFF data.
/// Numeric glyph IDs, `gid123`, and mapped Unicode names such as `uni0041`
/// are also accepted, following HarfBuzz's glyph-from-string fallbacks.
pub fn parse_glyph_names(
    font: &write_fonts::read::FontRef,
    input: &str,
) -> Result<IntSet<GlyphId>, SubsetError> {
    use skrifa::MetadataProvider;
    if input.trim() == "*" {
        return Ok(IntSet::all());
    }
    if input.trim().is_empty() {
        return Ok(IntSet::empty());
    }
    let names: crate::FastHashMap<_, _> = font
        .glyph_names()
        .iter()
        .map(|(gid, name)| (name.as_str().to_owned(), gid))
        .collect();
    let charmap = font.charmap();
    input
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|name| !name.is_empty())
        .map(|name| {
            names
                .get(name)
                .copied()
                .or_else(|| name.parse::<u32>().ok().map(GlyphId::new))
                .or_else(|| {
                    name.strip_prefix("gid")?
                        .parse::<u32>()
                        .ok()
                        .map(GlyphId::new)
                })
                .or_else(|| {
                    let cp = u32::from_str_radix(name.strip_prefix("uni")?, 16).ok()?;
                    charmap.map(cp)
                })
                .ok_or_else(|| SubsetError::InvalidGlyphName(name.into()))
        })
        .collect()
}

/// Parse comma-separated original:new glyph ID pairs, as in `1:4,2:7`.
pub fn parse_glyph_mapping(input: &str) -> Result<Vec<(GlyphId, GlyphId)>, SubsetError> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    input
        .split(',')
        .map(|pair| {
            let invalid = || SubsetError::InvalidGlyphMapping(pair.into());
            let (old, new) = pair.trim().split_once(':').ok_or_else(invalid)?;
            let old = old.trim().parse::<u32>().map_err(|_| invalid())?;
            let new = new.trim().parse::<u32>().map_err(|_| invalid())?;
            Ok((GlyphId::new(old), GlyphId::new(new)))
        })
        .collect()
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
    let unicode_str: String = unicode_str
        .chars()
        // Similar to fonttools, but 'x' and 'X' are left for `parse_hex` to deal with. This does
        // mean we support things like "x12", but not "xx12".
        .map(|c| match c {
            '>' | '<' | '+' | ',' | ';' | '&' | '#' | '{' | '}' | '\\' | 'u' | 'U' | 'n' | 'N'
            | 'i' | 'I' | '\n' | '\t' | '\x0B' | '\x0C' | '\r' => ' ',
            _ => c,
        })
        .collect();
    for cp in unicode_str.split_whitespace() {
        if let Some((start, end)) = cp.split_once('-') {
            let (start, end) = (parse_hex(start)?, parse_hex(end)?);
            if start > end {
                return Err(SubsetError::InvalidUnicodeRange { start, end });
            }
            result.extend(start..=end);
        } else {
            result.insert(parse_hex(cp)?);
        }
    }
    Ok(result)
}

fn parse_hex(hex: &str) -> Result<u32, SubsetError> {
    let hex = hex
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .trim_start_matches("x")
        .trim_start_matches("X");
    u32::from_str_radix(hex, 16).map_err(|_| SubsetError::InvalidUnicode(hex.to_owned()))
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

    let output = parse_unicodes("0x61,0x65-67").unwrap();
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

#[test]
fn glyph_names_use_post_cff_and_harfbuzz_string_fallbacks() {
    use write_fonts::read::FontRef;
    let source = std::fs::read("test-data/fonts/AlegreyaSans-BlackItalic.ttf").unwrap();
    let font = FontRef::new(&source).unwrap();
    let glyphs = parse_glyph_names(&font, ".notdef, A gid31 uni0043 c").unwrap();
    assert_eq!(
        glyphs,
        [0, 3, 31, 32, 284].into_iter().map(GlyphId::new).collect()
    );
    assert!(parse_glyph_names(&font, "*")
        .unwrap()
        .contains(GlyphId::new(1000)));
    assert!(parse_glyph_names(&font, "").unwrap().is_empty());
    assert!(matches!(
        parse_glyph_names(&font, "missing"),
        Err(SubsetError::InvalidGlyphName(_))
    ));
    assert!(parse_glyph_names(&font, "uniFFFF").is_err());
    let source = std::fs::read("test-data/fonts/Roboto-Regular.abc.ttf").unwrap();
    let font = FontRef::new(&source).unwrap();
    assert_eq!(
        parse_glyph_names(&font, "0 gid1 uni0062 3").unwrap(),
        (0..=3).map(GlyphId::new).collect()
    );
    assert!(parse_glyph_names(&font, "a").is_err());
    let source = std::fs::read("test-data/fonts/cff1_seac.otf").unwrap();
    let font = FontRef::new(&source).unwrap();
    assert_eq!(
        parse_glyph_names(&font, ".notdef A U grave dieresis").unwrap(),
        [0, 1, 2, 5, 6].into_iter().map(GlyphId::new).collect()
    );
}
