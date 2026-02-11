//! Type1 font parsing.

/// Raw dictionary data for a Type1 font.
struct RawDicts<'a> {
    /// Data containing the base dicitionary.
    base: &'a [u8],
    /// Data containing the decrypted private dictionary.
    private: Vec<u8>,
}

impl<'a> RawDicts<'a> {
    fn new(data: &'a [u8]) -> Option<Self> {
        if let Some((PFB_TEXT_SEGMENT_TAG, base_size)) = decode_pfb_tag(data, 0) {
            // We have a PFB; skip the tag
            let data = data.get(6..)?;
            verify_header(data)?;
            let (base_dict, raw_private_dict) = data.split_at_checked(base_size as usize)?;
            // Decrypt private dict segments
            let mut private_dict = decrypt(
                decode_pfb_binary_segments(raw_private_dict)
                    .flat_map(|segment| segment.iter().copied()),
                EEXEC_SEED,
            )
            // First four bytes are random garbage
            .skip(4)
            .collect::<Vec<_>>();
            Some(Self {
                base: base_dict,
                private: private_dict,
            })
        } else {
            // We have a PFA
            verify_header(data)?;
            // Now find the start of the private dictionary
            let start = find_eexec_data(data)?;
            let (base_dict, raw_private_dict) = data.split_at_checked(start)?;
            let mut private_dict = if raw_private_dict.len() > 3
                && raw_private_dict[..4].iter().all(|b| b.is_ascii_hexdigit())
            {
                // Hex decode and then decrypt
                decrypt(decode_hex(raw_private_dict.iter().copied()), EEXEC_SEED)
                    .skip(4)
                    .collect::<Vec<_>>()
            } else {
                // Just decrypt
                decrypt(raw_private_dict.iter().copied(), EEXEC_SEED)
                    .skip(4)
                    .collect::<Vec<_>>()
            };
            Some(Self {
                base: base_dict,
                private: private_dict,
            })
        }
    }
}

fn verify_header(data: &[u8]) -> Option<()> {
    (data.starts_with(b"%!PS-AdobeFont") || data.starts_with(b"%!FontType")).then_some(())
}

const PFB_TEXT_SEGMENT_TAG: u16 = 0x8001;
const PFB_BINARY_SEGMENT_TAG: u16 = 0x8002;

/// Returns the PFB tag and segment size.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/type1/t1parse.c#L69>
fn decode_pfb_tag(data: &[u8], start: usize) -> Option<(u16, u32)> {
    let header: [u8; 6] = data.get(start..start + 6)?.try_into().ok()?;
    let tag = (header[0] as u16) << 8 | header[1] as u16;
    if matches!(tag, PFB_BINARY_SEGMENT_TAG | PFB_TEXT_SEGMENT_TAG) {
        let size = u32::from_le_bytes(header[2..].try_into().unwrap());
        Some((tag, size))
    } else {
        None
    }
}

/// Returns an iterator over the sequence of PFB binary segments.
fn decode_pfb_binary_segments<'a>(data: &'a [u8]) -> impl Iterator<Item = &'a [u8]> + 'a {
    let mut pos = 0usize;
    core::iter::from_fn(move || {
        let (tag, len) = decode_pfb_tag(data, pos)?;
        // FT only decodes the sequence of binary segments here
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/type1/t1parse.c#L286>
        if tag != PFB_BINARY_SEGMENT_TAG {
            return None;
        }
        // Skip tag and size bytes
        let start = pos + 6;
        let end = start + len as usize;
        let segment = data.get(start..end)?;
        pos = end;
        Some(segment)
    })
}

/// Helper to find the position of the data following the 'eexec' token.
///
/// Unsurprisingly, more complicated than it should be.
fn find_eexec_data(data: &[u8]) -> Option<usize> {
    for (i, ch) in data.iter().enumerate() {
        // 5 letters for "eexec" plus 1 space plus 4 bytes
        const MIN_LEN: usize = 9;
        if *ch == b'e' && i + MIN_LEN < data.len() && data.get(i..)?.starts_with(b"eexec") {
            // FreeType has some unfun logic for skipping whitespace
            // after the eexec token
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/type1/t1parse.c#L382>
            let mut start = i + 5;
            while start < data.len() {
                match data[start] {
                    b' ' | b'\t' | b'\n' => {}
                    b'\r' => {
                        // Only stop at \r if it is not followed by \n
                        if data.get(start + 1) != Some(&b'\n') {
                            break;
                        }
                    }
                    _ => break,
                }
                start += 1;
            }
            if start == data.len() {
                // eexec not properly terminated
                return None;
            }
            return Some(start);
        }
    }
    None
}

/// Converts hex formatted data to associated bytes.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psconv.c#L464>
fn decode_hex(mut bytes: impl Iterator<Item = u8>) -> impl Iterator<Item = u8> {
    /// Converts digits (as ASCII characters) into integer values.
    const DIGIT_TO_NUM: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, -1, -1, -1, -1, -1, -1, -1, 10, 11, 12, 13, 14, 15,
        16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, -1, -1, -1,
        -1, -1, -1, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
        30, 31, 32, 33, 34, 35, -1, -1, -1, -1, -1,
    ];
    let mut pad = 0x1_u32;
    core::iter::from_fn(move || {
        loop {
            let Some(c) = bytes.next() else {
                break;
            };
            if is_whitespace(c) {
                continue;
            }
            if c >= 0x80 {
                break;
            }
            let c = DIGIT_TO_NUM[(c & 0x7F) as usize] as u32;
            if c >= 16 {
                break;
            }
            pad = (pad << 4) | c;
            if pad & 0x100 != 0 {
                let res = pad as u8;
                pad = 0x1;
                return Some(res);
            } else {
                continue;
            }
        }
        if pad != 0x1 {
            let res = (pad << 4) as u8;
            pad = 0x1;
            return Some(res);
        }
        None
    })
}

/// Decryption seed for eexec segment.
const EEXEC_SEED: u32 = 55665;

/// Returns an iterator yielding the decrypted bytes.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psconv.c#L557>
fn decrypt(bytes: impl Iterator<Item = u8>, mut seed: u32) -> impl Iterator<Item = u8> {
    bytes.map(move |b| {
        let b = b as u32;
        let plain = b ^ (seed >> 8);
        seed = b.wrapping_add(seed).wrapping_mul(52845).wrapping_add(22719) & 0xFFFF;
        plain as u8
    })
}

fn is_whitespace(c: u8) -> bool {
    if c <= 32 {
        return matches!(c, b' ' | b'\n' | b'\r' | b'\t' | b'\0' | 0x0C);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pfb_tags() {
        // Text segment tag
        let data = [0x80, 0x01, 0x01, 0x02, 0x00, 0x00];
        let (tag, len) = decode_pfb_tag(&data, 0).unwrap();
        assert_eq!(tag, PFB_TEXT_SEGMENT_TAG);
        assert_eq!(len, 513);
        // Binary segment tag
        let data = [0x80, 0x02, 0x01, 0x03, 0x00, 0x00];
        let (tag, len) = decode_pfb_tag(&data, 0).unwrap();
        assert_eq!(tag, PFB_BINARY_SEGMENT_TAG);
        assert_eq!(len, 769);
        // Invalid tag
        let data = [0x00; 6];
        assert!(decode_pfb_tag(&data, 0).is_none());
        // Not enough data
        let data = [0x00; 5];
        assert!(decode_pfb_tag(&data, 0).is_none());
    }

    #[test]
    fn pfb_segments() {
        let segments = [
            vec![0x01; 8],
            vec![0x02; 10],
            vec![0x03; 4],
            vec![0x04; 255],
        ];
        // Write each segment to a buffer
        let mut buf = vec![];
        for segment in &segments {
            buf.push(0x80);
            buf.push(0x02);
            buf.push(segment.len() as u8);
            buf.extend_from_slice(&[0; 3]);
            for byte in segment {
                buf.push(*byte);
            }
        }
        // Now parse and compare
        let mut parsed_count = 0;
        for (parsed, expected) in decode_pfb_binary_segments(&buf).zip(&segments) {
            assert_eq!(parsed, expected);
            parsed_count += 1;
        }
        assert_eq!(parsed_count, segments.len());
    }

    #[test]
    fn hex_decode() {
        check_hex_decode(
            b"743F8413F3636CA85A9FFEFB50B4BB27",
            &[
                116, 63, 132, 19, 243, 99, 108, 168, 90, 159, 254, 251, 80, 180, 187, 39,
            ],
        );
    }

    #[test]
    fn hex_decode_ignores_whitespace() {
        check_hex_decode(
            b"743F 8413F3636C\nA85A9FFEF\tB50B     4BB27",
            &[
                116, 63, 132, 19, 243, 99, 108, 168, 90, 159, 254, 251, 80, 180, 187, 39,
            ],
        );
    }

    #[test]
    fn hex_decode_truncate() {
        check_hex_decode(b"743F.8413F3636CA85A9FFEFB50B4BB27", &[116, 63]);
    }

    #[test]
    fn hex_decode_odd_chars() {
        check_hex_decode(b"743", &[116, 48]);
    }

    fn check_hex_decode(hex: &[u8], expected: &[u8]) {
        let decoded = decode_hex(hex.iter().copied()).collect::<Vec<_>>();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn decrypt_bytes() {
        let cipher = [
            0x74, 0x3f, 0x84, 0x13, 0xf3, 0x63, 0x6c, 0xa8, 0x5a, 0x9f, 0xfe, 0xfb, 0x50, 0xb4,
            0xbb, 0x27,
        ];
        let plain = decrypt(cipher.iter().copied(), EEXEC_SEED).collect::<Vec<_>>();
        // First 4 bytes are random garbage
        assert_eq!(&plain[4..], b"dup\n/Private");
    }

    #[test]
    fn find_eexec() {
        // Just a space
        assert_eq!(
            find_eexec_data(b"dup\n/Private\ncurrentfile eexec *&&FW"),
            Some(31)
        );
        // Multiple spaces
        assert_eq!(
            find_eexec_data(b"dup\n/Private\ncurrentfile eexec     *&&FW"),
            Some(35)
        );
        // New lines
        assert_eq!(
            find_eexec_data(b"dup\n/Private\ncurrentfile eexec\n\n*&&FW"),
            Some(32)
        );
        // Only skip \r when it precedes \n
        assert_eq!(
            find_eexec_data(b"dup\n/Private\ncurrentfile eexec\r\n\r*&&FW"),
            Some(32)
        );
    }

    #[test]
    fn read_pfb_raw_dicts() {
        let dicts = RawDicts::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFB).unwrap();
        check_noto_serif_base(dicts.base);
        check_noto_serif_private(&dicts.private);
    }

    #[test]
    fn read_pfa_raw_dicts() {
        let dicts = RawDicts::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFA).unwrap();
        check_noto_serif_base(dicts.base);
        check_noto_serif_private(&dicts.private);
    }

    fn check_noto_serif_base(base: &[u8]) {
        const EXPECTED_PREFIX: &str = r#"%!PS-AdobeFont-1.0: NotoSerif-Regular 2.007; ttfautohint (v1.8) -l 8 -r 50 -G 200 -x 14 -D latn -f none -a qsq -X ""
%%Title: NotoSerif-Regular
%Version: 2.007; ttfautohint (v1.8) -l 8 -r 50 -G 200 -x 14 -D latn -f none -a qsq -X ""
%%CreationDate: Tue Feb 10 16:07:25 2026
%%Creator: www-data
%Copyright: Copyright 2015-2021 Google LLC. All Rights Reserved.
% Generated by FontForge 20190801 (http://fontforge.sf.net/)
%%EndComments

10 dict begin
/FontType 1 def
/FontMatrix [0.001 0 0 0.001 0 0 ]readonly def
/FontName /NotoSerif-Regular def
/FontBBox {5 0 989 775 }readonly def
"#;
        assert!(base.starts_with(EXPECTED_PREFIX.as_bytes()));
    }

    fn check_noto_serif_private(private: &[u8]) {
        const EXPECTED_PREFIX: &str = r#"dup
/Private 8 dict dup begin
/RD{string currentfile exch readstring pop}executeonly def
/ND{noaccess def}executeonly def
/NP{noaccess put}executeonly def
/MinFeature{16 16}ND
/password 5839 def
/BlueValues [0 0 536 536 714 714 770 770 ]ND
/OtherSubrs"#;
        assert!(private.starts_with(EXPECTED_PREFIX.as_bytes()))
    }
}
