//! Container-level parsing, validation, and serialization (SFNT and TTC).

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};

const MAX_FILE_SIZE: usize = 1024 * 1024 * 1024; // 1 GB
const TTC_TAG: Tag = Tag::new(b"ttcf");
const TRUE_TAG: Tag = Tag::new(b"true");
const OTTO_TAG: Tag = Tag::new(b"OTTO");
const TT_VERSION: u32 = 0x00010000;

/// Information about a raw table entry found in an SFNT table directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawTableEntry {
    pub tag: Tag,
    pub checksum: u32,
    pub offset: u32,
    pub length: u32,
}

/// Parsed SFNT header and directory.
#[derive(Debug, Clone)]
pub struct SfntDirectory {
    pub sfnt_version: u32,
    pub num_tables: u16,
    pub search_range: u16,
    pub entry_selector: u16,
    pub range_shift: u16,
    pub tables: Vec<RawTableEntry>,
}

/// Check if a tag consists purely of printable ASCII characters (32..=126).
pub fn is_printable_tag(tag: Tag) -> bool {
    tag.into_bytes().iter().all(|&b| (32..=126).contains(&b))
}

impl SfntDirectory {
    /// Parse and validate an SFNT header and table directory at the specified `start_offset`.
    pub fn parse(
        data: &[u8],
        start_offset: usize,
        context: &mut dyn SanitizeContext,
    ) -> Result<Self, SanitizeError> {
        if data.len() > MAX_FILE_SIZE {
            return Err(SanitizeError::FileTooLarge);
        }

        if start_offset > data.len() {
            return Err(SanitizeError::Truncated("Offset beyond end of file"));
        }

        let slice = &data[start_offset..];
        if slice.len() < 12 {
            return Err(SanitizeError::Truncated("Cannot read SFNT header"));
        }

        let mut raw_version = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
        if raw_version == u32::from_be_bytes(TRUE_TAG.to_be_bytes()) {
            context.message(
                MessageLevel::Warning,
                "Normalizing 'true' sfntVersion to 0x00010000",
            );
            raw_version = TT_VERSION;
        } else if raw_version != TT_VERSION && raw_version != u32::from_be_bytes(OTTO_TAG.to_be_bytes()) {
            return Err(SanitizeError::InvalidSfntVersion(raw_version));
        }

        let num_tables = u16::from_be_bytes([slice[4], slice[5]]);
        let search_range = u16::from_be_bytes([slice[6], slice[7]]);
        let entry_selector = u16::from_be_bytes([slice[8], slice[9]]);
        let range_shift = u16::from_be_bytes([slice[10], slice[11]]);

        if !(1..4096).contains(&num_tables) {
            return Err(SanitizeError::InvalidTableCount(num_tables));
        }

        // Validate table directory search header formulas per OpenType spec
        let mut max_pow2 = 0u16;
        while (1u16 << (max_pow2 + 1)) <= num_tables {
            max_pow2 += 1;
        }
        let expected_search_range = (1u16 << max_pow2) << 4;
        let expected_entry_selector = max_pow2;
        let expected_range_shift = (num_tables * 16).saturating_sub(expected_search_range);

        if search_range != expected_search_range {
            context.message(
                MessageLevel::Warning,
                "Bad table directory searchRange; will be corrected in output",
            );
        }
        if entry_selector != expected_entry_selector {
            context.message(
                MessageLevel::Warning,
                "Bad table directory entrySelector; will be corrected in output",
            );
        }
        if range_shift != expected_range_shift {
            context.message(
                MessageLevel::Warning,
                "Bad table directory rangeShift; will be corrected in output",
            );
        }

        let directory_len = 12 + (num_tables as usize) * 16;
        if slice.len() < directory_len {
            return Err(SanitizeError::Truncated("Cannot read table directory"));
        }

        let mut tables = Vec::with_capacity(num_tables as usize);
        let mut prev_tag: Option<Tag> = None;

        for i in 0..num_tables as usize {
            let rec_offset = 12 + i * 16;
            let tag = Tag::new(&slice[rec_offset..rec_offset + 4].try_into().unwrap());
            let checksum = u32::from_be_bytes(
                slice[rec_offset + 4..rec_offset + 8].try_into().unwrap(),
            );
            let offset = u32::from_be_bytes(
                slice[rec_offset + 8..rec_offset + 12].try_into().unwrap(),
            );
            let length = u32::from_be_bytes(
                slice[rec_offset + 12..rec_offset + 16].try_into().unwrap(),
            );

            // Check tag ordering
            if let Some(prev) = prev_tag {
                if tag <= prev {
                    context.message(
                        MessageLevel::Warning,
                        &format!("Table directory is not correctly ordered ('{tag}' <= '{prev}')"),
                    );
                }
            }
            prev_tag = Some(tag);

            if !is_printable_tag(tag) {
                context.message(
                    MessageLevel::Warning,
                    &format!("Invalid or non-printable table tag: {tag:?}"),
                );
            }

            // Tables must be 4-byte aligned
            if offset & 3 != 0 {
                return Err(SanitizeError::MisalignedTable { tag, offset });
            }

            // Tables must start after the directory in this subfont or within the container
            if (offset as usize) < start_offset + directory_len && start_offset == 0 {
                return Err(SanitizeError::InvalidTableOffset { tag, offset });
            }
            if offset as usize >= data.len() {
                return Err(SanitizeError::InvalidTableOffset { tag, offset });
            }

            if length == 0 {
                return Err(SanitizeError::ZeroLengthTable(tag));
            }
            if length as usize > MAX_FILE_SIZE {
                return Err(SanitizeError::TableTooLarge { tag, length });
            }

            let end = (offset as usize).saturating_add(length as usize);
            if end > data.len() {
                return Err(SanitizeError::TableOverrunsFile {
                    tag,
                    offset,
                    length,
                    file_size: data.len(),
                });
            }

            tables.push(RawTableEntry {
                tag,
                checksum,
                offset,
                length,
            });
        }

        // Overlap checking: verify tables do not overlap each other
        let mut intervals = Vec::with_capacity(tables.len() * 2);
        for entry in &tables {
            intervals.push((entry.offset, 1i32));
            intervals.push((entry.offset + entry.length, -1i32));
        }
        intervals.sort_unstable_by(|a, b| {
            if a.0 != b.0 {
                a.0.cmp(&b.0)
            } else {
                // If an end and start coincide at same offset, process end (-1) before start (1)
                a.1.cmp(&b.1)
            }
        });

        let mut active_count = 0i32;
        for (_, delta) in intervals {
            active_count += delta;
            if active_count > 1 {
                return Err(SanitizeError::OverlappingTables);
            }
        }

        Ok(Self {
            sfnt_version: raw_version,
            num_tables,
            search_range,
            entry_selector,
            range_shift,
            tables,
        })
    }
}

/// Parsed TTC collection header.
#[derive(Debug, Clone)]
pub struct TtcHeader {
    pub version: u32,
    pub num_fonts: u32,
    pub font_offsets: Vec<u32>,
}

impl TtcHeader {
    /// Detect and parse a TTC header if data starts with `'ttcf'`.
    pub fn try_parse(data: &[u8]) -> Result<Option<Self>, SanitizeError> {
        if data.len() < 4 {
            return Ok(None);
        }
        let tag = Tag::new(&data[0..4].try_into().unwrap());
        if tag != TTC_TAG {
            return Ok(None);
        }

        if data.len() < 12 {
            return Err(SanitizeError::Truncated("TTC header too short"));
        }

        let version = u32::from_be_bytes(data[4..8].try_into().unwrap());
        if version != 0x00010000 && version != 0x00020000 {
            return Err(SanitizeError::InvalidTtc("Unsupported TTC version"));
        }

        let num_fonts = u32::from_be_bytes(data[8..12].try_into().unwrap());
        if num_fonts > 0x10000 {
            return Err(SanitizeError::InvalidTtc("Too many fonts in TTC"));
        }

        let required_len = 12 + (num_fonts as usize) * 4;
        if data.len() < required_len {
            return Err(SanitizeError::Truncated("Cannot read TTC font offsets"));
        }

        let mut font_offsets = Vec::with_capacity(num_fonts as usize);
        for i in 0..num_fonts as usize {
            let off = 12 + i * 4;
            let offset = u32::from_be_bytes(data[off..off + 4].try_into().unwrap());
            font_offsets.push(offset);
        }

        Ok(Some(Self {
            version,
            num_fonts,
            font_offsets,
        }))
    }
}
