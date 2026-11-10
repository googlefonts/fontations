//! Type1 font test data.

pub static NOTO_SERIF_REGULAR_SUBSET_PFA: &[u8] =
    include_bytes!("../test_data/type1/notoserif-regular.subset.pfa");

pub static NOTO_SERIF_REGULAR_SUBSET_PFB: &[u8] =
    include_bytes!("../test_data/type1/notoserif-regular.subset.pfb");

pub static HANGING_S_CID: &[u8] = include_bytes!("../test_data/type1/HangingS.cid");

pub static HANGING_S_HEX_CID: &[u8] = include_bytes!("../test_data/type1/HangingS_hex.cid");

pub static ADOBE_CIDFONT_PS: &[u8] = include_bytes!("../test_data/type1/cidfont.ps");

/// A CID Type 1 resource with two font dictionaries, different matrices,
/// subroutine bodies and encryption settings, a missing CID, and an empty CID.
/// Constructed here so tests can vary its dictionaries and binary maps.
pub fn cid_font(hex: bool) -> Vec<u8> {
    fn encrypt(bytes: &[u8]) -> Vec<u8> {
        let mut seed = 4330_u32;
        [0; 4]
            .iter()
            .chain(bytes)
            .map(|&byte| {
                let cipher = byte ^ (seed >> 8) as u8;
                seed = ((cipher as u32 + seed) * 52845 + 22719) & 0xffff;
                cipher
            })
            .collect()
    }
    fn set_offset(bytes: &mut [u8], pos: usize, value: usize) {
        bytes[pos..pos + 4].copy_from_slice(&(value as u32).to_be_bytes());
    }
    // Five CID map entries, including the sentinel, then two subroutine maps.
    let mut binary = vec![0; 25 + 8 + 8];
    // Subroutine zero draws two sides of a triangle and returns.
    let subr0 = encrypt(&[159, 139, 5, 139, 169, 5, 11]);
    let subr1 = [179, 139, 5, 139, 189, 5, 11];
    set_offset(&mut binary, 25, 41);
    binary.extend_from_slice(&subr0);
    let end = binary.len();
    set_offset(&mut binary, 29, end);
    set_offset(&mut binary, 33, end);
    binary.extend_from_slice(&subr1);
    let end = binary.len();
    set_offset(&mut binary, 37, end);
    // 0 600 hsbw 0 0 rmoveto 0 callsubr endchar
    let glyph = [139, 248, 236, 13, 139, 139, 21, 139, 10, 14];
    set_offset(&mut binary, 1, end);
    binary.extend_from_slice(&encrypt(&glyph));
    let end = binary.len();
    binary[5] = 1;
    set_offset(&mut binary, 6, end);
    binary.extend_from_slice(&glyph);
    let end = binary.len();
    binary[10] = 255; // Missing glyph, as used by real CID fonts.
    for pos in [11, 16, 21] {
        set_offset(&mut binary, pos, end);
    }

    let mut result = format!(
        r"%!PS-Adobe-3.0 Resource-CIDFont
/CIDInit /ProcSet findresource begin
20 dict begin
/CIDFontName /TwoDicts def
/CIDFontType 0 def
/CIDSystemInfo 3 dict dup begin
/Registry (Adobe) def /Ordering (Identity) def /Supplement 0 def
end def
/FontBBox [0 0 200 100] def
/FontInfo 2 dict dup begin /FamilyName (Two Dicts) def /Weight (Regular) def end def
/CIDCount 4 def
/CIDMapOffset 0 def /FDBytes 1 def /GDBytes 4 def
/FDArray 2 array
dup 0
%ADOBeginFontDict
5 dict dup begin
/FontType 1 def /FontMatrix [0.001 0 0 0.001 0 0] def
/Private 4 dict dup begin
/lenIV 4 def /SubrMapOffset 25 def /SDBytes 4 def /SubrCount 1 def
end def end put
%ADOEndFontDict
dup 1
%ADOBeginFontDict
5 dict dup begin
/FontType 1 def /FontMatrix [0.002 0 0.0005 0.001 0.01 0.02] def
/Private 4 dict dup begin
/lenIV -1 def /SubrMapOffset 33 def /SDBytes 4 def /SubrCount 1 def
end def end put
%ADOEndFontDict
def
({}) {} StartData ",
        if hex { "Hex" } else { "Binary" },
        binary.len()
    )
    .into_bytes();
    if hex {
        use std::fmt::Write;
        let mut text = String::new();
        for byte in binary {
            write!(text, "{byte:02X}").unwrap();
        }
        result.extend_from_slice(text.as_bytes());
        result.push(b'>');
    } else {
        result.extend_from_slice(&binary);
    }
    result.extend_from_slice(b"\n%%EndData\nend\nend\n");
    result
}
