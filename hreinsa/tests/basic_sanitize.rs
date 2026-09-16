use font_types::Tag;
use hreinsa::{sanitize, SanitizeError};
use read_fonts::{FontRef, TableProvider};

#[test]
fn test_sanitize_tofu() {
    let font_data = font_test_data::TOFU;
    let sanitized = sanitize(font_data).expect("Failed to sanitize TOFU font");

    // Check that sanitized bytes are a valid OpenType font
    let font = FontRef::new(&sanitized).expect("Sanitized output should be readable by FontRef");
    assert!(font.head().is_ok());
    assert!(font.maxp().is_ok());
    assert!(font.os2().is_ok());
    assert!(font.cmap().is_ok());
    assert!(font.hhea().is_ok());
    assert!(font.hmtx().is_ok());
    assert!(font.name().is_ok());
    assert!(font.post().is_ok());
    assert!(font.loca(None).is_ok());
    assert!(font.glyf().is_ok());
}

#[test]
fn test_sanitize_noto_serif() {
    let font_data = font_test_data::NOTO_SERIF_DISPLAY_TRIMMED;
    let sanitized = sanitize(font_data).expect("Failed to sanitize NOTO_SERIF font");
    let font = FontRef::new(&sanitized).expect("Sanitized output should be readable by FontRef");
    assert!(font.head().is_ok());
    assert!(font.maxp().is_ok());
}

#[test]
fn test_sanitize_rejects_missing_required_table() {
    // VAZIRMATN_VAR is missing OS/2
    let font_data = font_test_data::VAZIRMATN_VAR;
    assert_eq!(
        sanitize(font_data),
        Err(SanitizeError::MissingRequiredTable(Tag::new(b"OS/2")))
    );
}

#[test]
fn test_sanitize_cff_font() {
    let font_data = font_test_data::NOTO_SANS_JP_CFF;
    let sanitized = sanitize(font_data).expect("Failed to sanitize CFF font");
    let font = FontRef::new(&sanitized).expect("Sanitized output should be readable by FontRef");
    assert!(font.head().is_ok());
    assert!(font.maxp().is_ok());
    let maxp = font.maxp().unwrap();
    assert_eq!(maxp.version(), font_types::Version16Dot16::VERSION_0_5);
}

#[test]
fn test_sanitize_rejects_truncated_file() {
    let truncated = &font_test_data::TOFU[..8];
    assert!(matches!(
        sanitize(truncated),
        Err(SanitizeError::Truncated(_))
    ));
}

#[test]
fn test_sanitize_rejects_empty_file() {
    assert!(matches!(
        sanitize(&[]),
        Err(SanitizeError::Truncated(_))
    ));
}

#[test]
fn test_sanitize_rejects_invalid_sfnt_version() {
    let mut bad_data = font_test_data::TOFU.to_vec();
    bad_data[0..4].copy_from_slice(&0x12345678u32.to_be_bytes());
    assert!(matches!(
        sanitize(&bad_data),
        Err(SanitizeError::InvalidSfntVersion(0x12345678))
    ));
}

#[test]
fn test_sanitize_masks_head_flags() {
    let mut data = font_test_data::TOFU.to_vec();
    // Locate head table offset in directory
    let font = FontRef::new(&data).unwrap();
    let head_entry = font
        .table_directory()
        .table_records()
        .iter()
        .find(|r| r.tag() == Tag::new(b"head"))
        .unwrap();
    let head_offset = head_entry.offset() as usize;

    // Set bit 5 (0x0020, which is reserved and should be cleared by sanitization)
    data[head_offset + 16] = 0xFF;
    data[head_offset + 17] = 0xFF;

    let sanitized = sanitize(&data).expect("Sanitizing should succeed and mask reserved bits");
    let sanitized_font = FontRef::new(&sanitized).unwrap();
    let head = sanitized_font.head().unwrap();
    assert_eq!(head.flags().bits() & 0x0020, 0);
}
