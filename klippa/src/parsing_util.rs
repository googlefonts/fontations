//! subsetter input parsing util functions
use int_set::IntSet;
use write_fonts::types::GlyphId;

use crate::SubsetError;
pub fn populate_gids(gid_str: &str) -> Result<IntSet<GlyphId>, SubsetError> {
    let mut result = IntSet::empty();

    for gid in gid_str.split(',') {
        if let Some((start, end)) = gid.split_once('-') {
            let start: u16 = start
                .parse::<u16>()
                .map_err(|_| SubsetError::InvalidGid(start.to_owned()))?;
            let end: u16 = end
                .parse::<u16>()
                .map_err(|_| SubsetError::InvalidGid(end.to_owned()))?;
            if start > end {
                return Err(SubsetError::InvalidGidRange { start, end });
            }
            result.extend((start..=end).map(GlyphId::new));
        } else {
            let glyph_id: u16 = gid
                .parse::<u16>()
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
    if unicode_str == "*" {
        let out = IntSet::<u32>::all();
        return Ok(out);
    }
    let mut result = IntSet::empty();
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

#[test]
fn test_populate_gids() {
    let input = "1,5,7";
    let output = populate_gids(input).unwrap();
    //assert_eq!(output.len(), 3);
    assert!(output.contains(GlyphId::new(1)));
    assert!(output.contains(GlyphId::new(5)));
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
