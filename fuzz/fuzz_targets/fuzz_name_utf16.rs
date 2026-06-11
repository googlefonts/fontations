//! Differential fuzzing of the name table's UTF-16BE decoder.
//!
//! Wraps the raw fuzz input in a minimal name table, decodes it through
//! `NameString::chars` and checks the result against the standard library's
//! lossy UTF-16 decoding.
#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use read_fonts::{tables::name::Name, FontData, FontRead};

/// Build a name table with a single record (platform 0 -> UTF-16BE) whose
/// string storage is exactly `data`.
fn make_name_table(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(18 + data.len());
    buf.extend_from_slice(&0u16.to_be_bytes()); // version
    buf.extend_from_slice(&1u16.to_be_bytes()); // count
    buf.extend_from_slice(&18u16.to_be_bytes()); // storage offset
    buf.extend_from_slice(&0u16.to_be_bytes()); // platform id (unicode)
    buf.extend_from_slice(&3u16.to_be_bytes()); // encoding id
    buf.extend_from_slice(&0u16.to_be_bytes()); // language id
    buf.extend_from_slice(&1u16.to_be_bytes()); // name id
    buf.extend_from_slice(&(data.len() as u16).to_be_bytes()); // length
    buf.extend_from_slice(&0u16.to_be_bytes()); // string offset
    buf.extend_from_slice(data);
    buf
}

/// Spec-correct lossy UTF-16BE decoding: unpaired surrogates and a dangling
/// trailing byte each become U+FFFD.
fn decode_utf16be_lossy(data: &[u8]) -> Vec<char> {
    let units = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes(c.try_into().unwrap()));
    let mut chars: Vec<char> = char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if data.len() % 2 != 0 {
        chars.push(char::REPLACEMENT_CHARACTER);
    }
    chars
}

fn compare_decodings(data: &[u8]) -> Result<(), Box<dyn Error>> {
    if data.len() > u16::MAX as usize {
        return Ok(());
    }
    let table = make_name_table(data);
    let name = Name::read(FontData::new(&table))?;
    let record = name.name_record().first().ok_or("missing record")?;
    let chars: Vec<char> = record.string(name.string_data())?.chars().collect();
    assert_eq!(chars, decode_utf16be_lossy(data));
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    compare_decodings(data).unwrap();
});
