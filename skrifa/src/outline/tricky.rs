//! Match FreeType's "tricky" font detection.
//!
//! Tricky fonts are those that have busted outlines and require the bytecode
//! interpreter to produce something that makes sense.

use crate::{string::StringId, FontRef, MetadataProvider};

pub(super) fn is_tricky(font: &FontRef) -> bool {
    has_tricky_name(font)
}

fn has_tricky_name(font: &FontRef) -> bool {
    font.localized_strings(StringId::FAMILY_NAME)
        .english_or_first()
        .map(|name| {
            let mut buf = [0u8; MAX_TRICKY_NAME_LEN];
            let mut len = 0;
            let mut chars = name.chars();
            for ch in chars.by_ref().take(MAX_TRICKY_NAME_LEN) {
                buf[len] = ch as u8;
                len += 1;
            }
            if chars.next().is_some() {
                return false;
            }
            is_tricky_name(core::str::from_utf8(&buf[..len]).unwrap_or_default())
        })
        .unwrap_or_default()
}

/// Does this family name belong to a "tricky" font?
///
/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/truetype/ttobjs.c#L174>
fn is_tricky_name(name: &str) -> bool {
    let name = skip_pdf_random_tag(name);
    TRICKY_NAMES
        .iter()
        // FreeType uses strstr(name, tricky_name) so we use contains() to
        // match behavior.
        .any(|tricky_name| name.contains(*tricky_name))
}

/// Fonts embedded in PDFs add random prefixes. Strip these
/// for tricky font comparison purposes.
///
/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/truetype/ttobjs.c#L153>
fn skip_pdf_random_tag(name: &str) -> &str {
    let bytes = name.as_bytes();
    // Random tag is 6 uppercase letters followed by a +
    if bytes.len() < 8 || bytes[6] != b'+' || !bytes.iter().take(6).all(|b| b.is_ascii_uppercase())
    {
        return name;
    }
    core::str::from_utf8(&bytes[7..]).unwrap_or(name)
}

/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/truetype/ttobjs.c#L180>
#[rustfmt::skip]
const TRICKY_NAMES: &[&str] = &[
    "cpop",               /* dftt-p7.ttf; version 1.00, 1992 [DLJGyShoMedium] */
    "DFGirl-W6-WIN-BF",   /* dftt-h6.ttf; version 1.00, 1993 */
    "DFGothic-EB",        /* DynaLab Inc. 1992-1995 */
    "DFGyoSho-Lt",        /* DynaLab Inc. 1992-1995 */
    "DFHei",              /* DynaLab Inc. 1992-1995 [DFHei-Bd-WIN-HK-BF] */
                          /* covers "DFHei-Md-HK-BF", maybe DynaLab Inc. */

    "DFHSGothic-W5",      /* DynaLab Inc. 1992-1995 */
    "DFHSMincho-W3",      /* DynaLab Inc. 1992-1995 */
    "DFHSMincho-W7",      /* DynaLab Inc. 1992-1995 */
    "DFKaiSho-SB",        /* dfkaisb.ttf */
    "DFKaiShu",           /* covers "DFKaiShu-Md-HK-BF", maybe DynaLab Inc. */
    "DFKai-SB",           /* kaiu.ttf; version 3.00, 1998 [DFKaiShu-SB-Estd-BF] */

    "DFMing",             /* DynaLab Inc. 1992-1995 [DFMing-Md-WIN-HK-BF] */
                          /* covers "DFMing-Bd-HK-BF", maybe DynaLab Inc. */

    "DLC",                /* dftt-m7.ttf; version 1.00, 1993 [DLCMingBold] */
                          /* dftt-f5.ttf; version 1.00, 1993 [DLCFongSung] */
                          /* covers following */
                          /* "DLCHayMedium", dftt-b5.ttf; version 1.00, 1993 */
                          /* "DLCHayBold",   dftt-b7.ttf; version 1.00, 1993 */
                          /* "DLCKaiMedium", dftt-k5.ttf; version 1.00, 1992 */
                          /* "DLCLiShu",     dftt-l5.ttf; version 1.00, 1992 */
                          /* "DLCRoundBold", dftt-r7.ttf; version 1.00, 1993 */

    "HuaTianKaiTi?",      /* htkt2.ttf */
    "HuaTianSongTi?",     /* htst3.ttf */
    "Ming(for ISO10646)", /* hkscsiic.ttf; version 0.12, 2007 [Ming] */
                          /* iicore.ttf; version 0.07, 2007 [Ming] */
    "MingLiU",            /* mingliu.ttf */
                          /* mingliu.ttc; version 3.21, 2001 */
    "MingMedium",         /* dftt-m5.ttf; version 1.00, 1993 [DLCMingMedium] */
    "PMingLiU",           /* mingliu.ttc; version 3.21, 2001 */
    "MingLi43",           /* mingli.ttf; version 1.00, 1992 */
];

const MAX_TRICKY_NAME_LEN: usize = 18;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_max_tricky_name_len() {
        let max_len = TRICKY_NAMES.iter().fold(0, |acc, name| acc.max(name.len()));
        assert_eq!(max_len, MAX_TRICKY_NAME_LEN);
    }

    #[test]
    fn skip_pdf_tags() {
        // length must be at least 8
        assert_eq!(skip_pdf_random_tag("ABCDEF+"), "ABCDEF+");
        // first six chars must be ascii uppercase
        assert_eq!(skip_pdf_random_tag("AbCdEF+Arial"), "AbCdEF+Arial");
        // no numbers
        assert_eq!(skip_pdf_random_tag("Ab12EF+Arial"), "Ab12EF+Arial");
        // missing +
        assert_eq!(skip_pdf_random_tag("ABCDEFArial"), "ABCDEFArial");
        // too long
        assert_eq!(skip_pdf_random_tag("ABCDEFG+Arial"), "ABCDEFG+Arial");
        // too short
        assert_eq!(skip_pdf_random_tag("ABCDE+Arial"), "ABCDE+Arial");
        // just right
        assert_eq!(skip_pdf_random_tag("ABCDEF+Arial"), "Arial");
    }

    #[test]
    fn all_tricky_names() {
        for name in TRICKY_NAMES {
            assert!(is_tricky_name(name));
        }
    }

    #[test]
    fn non_tricky_names() {
        for not_tricky in ["Roboto", "Arial", "Helvetica", "Blah", ""] {
            assert!(!is_tricky_name(not_tricky));
        }
    }
}
