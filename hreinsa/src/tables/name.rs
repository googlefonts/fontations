//! Sanitizer for the `name` (Naming Table) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};

pub const TAG: Tag = Tag::new(b"name");

fn is_uri_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~'
}

fn sanitize_ps_name_ascii(bytes: &mut [u8]) -> bool {
    if bytes.len() > 63 {
        return false;
    }
    for b in bytes.iter_mut() {
        if !is_uri_unreserved(*b) {
            *b = b'_';
        }
    }
    true
}

fn sanitize_ps_name_utf16be(bytes: &mut [u8]) -> bool {
    if !bytes.len().is_multiple_of(2) || bytes.len() > 126 {
        return false;
    }
    let (chunks, _) = bytes.as_chunks_mut::<2>();
    for chunk in chunks {
        if chunk[0] != 0 {
            // non-ASCII character in PS name
            return false;
        }
        if !is_uri_unreserved(chunk[1]) {
            chunk[1] = b'_';
        }
    }
    true
}

/// A parsed and sanitized name record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SanitizedNameRecord {
    pub platform_id: u16,
    pub encoding_id: u16,
    pub language_id: u16,
    pub name_id: u16,
    pub string_data: Vec<u8>,
}

/// Sanitized state of the `name` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameState {
    pub format: u16,
    pub records: Vec<SanitizedNameRecord>,
    pub lang_tags: Vec<Vec<u8>>,
}

impl NameState {
    pub fn parse(data: &[u8], context: &mut dyn SanitizeContext) -> Result<Self, SanitizeError> {
        if data.len() < 6 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for name header".into(),
            });
        }

        let format = u16::from_be_bytes(data[0..2].try_into().unwrap());
        if format > 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported name table format {format}"),
            });
        }

        let count = u16::from_be_bytes(data[2..4].try_into().unwrap()) as usize;
        let string_offset = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;

        if string_offset > data.len() {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!(
                    "stringOffset {string_offset} beyond table length {}",
                    data.len()
                ),
            });
        }

        let records_end = 6 + count * 12;
        if records_end > data.len() {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for name records".into(),
            });
        }

        let mut records = Vec::with_capacity(count);
        let mut was_sorted = true;

        for i in 0..count {
            let off = 6 + i * 12;
            let platform_id = u16::from_be_bytes(data[off..off + 2].try_into().unwrap());
            let encoding_id = u16::from_be_bytes(data[off + 2..off + 4].try_into().unwrap());
            let language_id = u16::from_be_bytes(data[off + 4..off + 6].try_into().unwrap());
            let name_id = u16::from_be_bytes(data[off + 6..off + 8].try_into().unwrap());
            let length = u16::from_be_bytes(data[off + 8..off + 10].try_into().unwrap()) as usize;
            let str_off =
                u16::from_be_bytes(data[off + 10..off + 12].try_into().unwrap()) as usize;

            // Validate platform & encoding per OTS rules
            let valid_platform = match platform_id {
                0 => encoding_id <= 6,
                1 => encoding_id <= 32,
                2 => encoding_id <= 2,
                3 => encoding_id <= 6 || encoding_id == 10,
                4 => encoding_id <= 255,
                _ => false,
            };

            if !valid_platform {
                continue;
            }

            let start = string_offset + str_off;
            let end = start + length;
            if end > data.len() {
                continue;
            }

            let mut str_bytes = data[start..end].to_vec();

            // Sanitize PostScript name (name_id 6)
            if name_id == 6 {
                if platform_id == 1 {
                    if !sanitize_ps_name_ascii(&mut str_bytes) {
                        continue;
                    }
                } else if (platform_id == 0 || platform_id == 3)
                    && !sanitize_ps_name_utf16be(&mut str_bytes)
                {
                    continue;
                }
            }

            let rec = SanitizedNameRecord {
                platform_id,
                encoding_id,
                language_id,
                name_id,
                string_data: str_bytes,
            };

            if let Some(last) = records.last() {
                if rec < *last {
                    was_sorted = false;
                }
            }
            records.push(rec);
        }

        if !was_sorted {
            context.message(
                MessageLevel::Warning,
                "name: records are not sorted; sorting them in output",
            );
            records.sort();
        }

        let mut lang_tags = Vec::new();
        if format == 1 {
            if records_end + 2 > data.len() {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: "Table too short for langTagCount in format 1 name table".into(),
                });
            }
            let lang_count =
                u16::from_be_bytes(data[records_end..records_end + 2].try_into().unwrap()) as usize;
            let tags_end = records_end + 2 + lang_count * 4;
            if tags_end > string_offset || tags_end > data.len() {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: "langTagRecords overlap string storage or table end".into(),
                });
            }

            for i in 0..lang_count {
                let off = records_end + 2 + i * 4;
                let length =
                    u16::from_be_bytes(data[off..off + 2].try_into().unwrap()) as usize;
                let tag_off =
                    u16::from_be_bytes(data[off + 2..off + 4].try_into().unwrap()) as usize;
                if length > 200 {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!("Language tag length {length} exceeds maximum (200)"),
                    });
                }
                let start = string_offset + tag_off;
                let end = start + length;
                if end > data.len() {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: "Language tag overruns table".into(),
                    });
                }
                lang_tags.push(data[start..end].to_vec());
            }
        }

        Ok(Self {
            format,
            records,
            lang_tags,
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let count = self.records.len();
        let header_len = if self.format == 1 {
            6 + count * 12 + 2 + self.lang_tags.len() * 4
        } else {
            6 + count * 12
        };

        let mut string_pool = Vec::new();
        let mut rec_offsets = Vec::with_capacity(count);

        for rec in &self.records {
            let offset = string_pool.len() as u16;
            string_pool.extend_from_slice(&rec.string_data);
            rec_offsets.push((offset, rec.string_data.len() as u16));
        }

        let mut lang_offsets = Vec::with_capacity(self.lang_tags.len());
        for tag in &self.lang_tags {
            let offset = string_pool.len() as u16;
            string_pool.extend_from_slice(tag);
            lang_offsets.push((offset, tag.len() as u16));
        }

        let mut bytes = Vec::with_capacity(header_len + string_pool.len());
        bytes.extend_from_slice(&self.format.to_be_bytes());
        bytes.extend_from_slice(&(count as u16).to_be_bytes());
        bytes.extend_from_slice(&(header_len as u16).to_be_bytes());

        for (rec, (str_off, str_len)) in self.records.iter().zip(rec_offsets) {
            bytes.extend_from_slice(&rec.platform_id.to_be_bytes());
            bytes.extend_from_slice(&rec.encoding_id.to_be_bytes());
            bytes.extend_from_slice(&rec.language_id.to_be_bytes());
            bytes.extend_from_slice(&rec.name_id.to_be_bytes());
            bytes.extend_from_slice(&str_len.to_be_bytes());
            bytes.extend_from_slice(&str_off.to_be_bytes());
        }

        if self.format == 1 {
            bytes.extend_from_slice(&(self.lang_tags.len() as u16).to_be_bytes());
            for (tag_off, tag_len) in lang_offsets {
                bytes.extend_from_slice(&tag_len.to_be_bytes());
                bytes.extend_from_slice(&tag_off.to_be_bytes());
            }
        }

        bytes.extend_from_slice(&string_pool);
        bytes
    }
}
