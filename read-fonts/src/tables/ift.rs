//! Incremental Font Transfer [Patch Map](https://w3c.github.io/IFT/Overview.html#font-format-extensions)

include!("../../generated/generated_ift.rs");

use core::str::Utf8Error;
use std::{ops::RangeInclusive, str};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U8Or16(u16);

impl ReadArgs for U8Or16 {
    type Args = u16;
}

impl ComputeSize for U8Or16 {
    fn compute_size(max_entry_index: &u16) -> Result<usize, ReadError> {
        Ok(if *max_entry_index < 256 { 1 } else { 2 })
    }
}

impl FontReadWithArgs<'_> for U8Or16 {
    fn read_with_args(data: FontData<'_>, max_entry_index: &Self::Args) -> Result<Self, ReadError> {
        if *max_entry_index < 256 {
            data.read_at::<u8>(0).map(|v| Self(v as u16))
        } else {
            data.read_at::<u16>(0).map(Self)
        }
    }
}

impl U8Or16 {
    #[inline]
    pub fn get(self) -> u16 {
        self.0
    }
}

impl<'a> PatchMapFormat1<'a> {
    pub fn get_compatibility_id(&self) -> [u32; 4] {
        [
            self.compatibility_id().first().unwrap().get(),
            self.compatibility_id().get(1).unwrap().get(),
            self.compatibility_id().get(2).unwrap().get(),
            self.compatibility_id().get(3).unwrap().get(),
        ]
    }

    pub fn gid_to_entry_iter(&'a self) -> impl Iterator<Item = (GlyphId, u16)> + 'a {
        GidToEntryIter {
            glyph_map: self.glyph_map().ok(),
            glyph_count: self.glyph_count(),
            gid: self
                .glyph_map()
                .map(|glyph_map| glyph_map.first_mapped_glyph() as u32)
                .unwrap_or(0),
        }
        .filter(|(_, entry_index)| *entry_index > 0)
    }

    pub fn entry_count(&self) -> u32 {
        self.max_entry_index() as u32 + 1
    }

    pub fn uri_template_as_string(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.uri_template())
    }

    pub fn is_entry_applied(&self, entry_index: u32) -> bool {
        let byte_index = entry_index / 8;
        let bit_mask = 1 << (entry_index % 8);
        self.applied_entries_bitmap()
            .get(byte_index as usize)
            .map(|byte| byte & bit_mask != 0)
            .unwrap_or(false)
    }

    pub fn entry_map_records(&self) -> impl Iterator<Item = FeatureEntryMapping> + 'a {
        let Some(Ok(feature_map)) = self.feature_map() else {
            return EntryMapIter::empty(&[]);
        };

        EntryMapIter {
            data: feature_map.entry_map_data(),
            feature_record_it: Some(feature_map.feature_records().iter()),
            current_feature_record: None,
            remaining: 0,
            max_entry_index: self.max_entry_index(),
        }
    }
}

struct GidToEntryIter<'a> {
    glyph_map: Option<GlyphMap<'a>>,
    glyph_count: u32,
    gid: u32,
}

impl<'a> Iterator for GidToEntryIter<'a> {
    type Item = (GlyphId, u16);

    fn next(&mut self) -> Option<Self::Item> {
        let glyph_map = self.glyph_map.as_ref()?;

        let cur_gid = self.gid;
        self.gid += 1;

        if cur_gid >= self.glyph_count {
            return None;
        }

        let index = cur_gid as usize - glyph_map.first_mapped_glyph() as usize;
        glyph_map
            .entry_index()
            .get(index)
            .ok()
            .map(|entry_index| (cur_gid.into(), entry_index.0))
    }
}

#[derive(PartialEq, Debug)]
pub struct FeatureEntryMapping {
    pub matched_entries: RangeInclusive<u16>,
    pub new_entry_index: u16,
    pub feature_tag: Tag,
}

struct EntryMapIter<'a, T>
where
    T: Iterator<Item = Result<FeatureRecord, ReadError>>,
{
    data: &'a [u8],
    feature_record_it: Option<T>,
    current_feature_record: Option<FeatureRecord>,
    remaining: u16,
    max_entry_index: u16,
}

impl<'a, T> EntryMapIter<'a, T>
where
    T: Iterator<Item = Result<FeatureRecord, ReadError>>,
{
    fn empty(data: &'a [u8]) -> Self {
        EntryMapIter {
            data,
            feature_record_it: None,
            current_feature_record: None,
            remaining: 0,
            max_entry_index: 0,
        }
    }
}

impl<T> Iterator for EntryMapIter<'_, T>
where
    T: Iterator<Item = Result<FeatureRecord, ReadError>>,
{
    type Item = FeatureEntryMapping;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO(garretrieger): current spec text for this is wrong and doesn't match what were doing here. Update
        //                     the spec to match this implementation.
        let feature_record_it = self.feature_record_it.as_mut()?;

        if self.current_feature_record.is_none() || self.remaining == 0 {
            let Some(Ok(feature_record)) = feature_record_it.next() else {
                self.feature_record_it = None;
                return None;
            };

            self.remaining = feature_record.entry_map_count().get();
            self.current_feature_record = Some(feature_record);
        }

        let data = FontData::new(self.data);
        let (Some(feature_record), Ok(entry_record)) = (
            self.current_feature_record.clone(),
            EntryMapRecord::read(data, self.max_entry_index),
        ) else {
            self.feature_record_it = None;
            return None;
        };

        let new_entry_index = feature_record.first_new_entry_index.get()
            + (feature_record.entry_map_count().get() - self.remaining);
        self.remaining -= 1;

        let size = U8Or16::compute_size(&self.max_entry_index).ok()? * 2;
        self.data = &self.data[size..];

        Some(FeatureEntryMapping {
            matched_entries: entry_record.first_entry_index().get()
                ..=entry_record.last_entry_index().get(),
            new_entry_index,
            feature_tag: feature_record.feature_tag(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data::ift as test_data;

    // TODO(garretrieger) - more tests (as functionality is implemented):
    // - Test where entryIndex array has len 0 (eg. all glyphs map to 0)
    // - Test which appliedEntriesBitmap > 1 byte
    // - Test w/ feature map populated.
    // - Test enforced minimum entry count of > 0.
    // - Test where entryIndex is a u16.
    // - Invalid table (too short).
    // - Invalid UTF8 sequence in uri template.
    // - Compat ID is to short.
    // - invalid entry map array (too short)
    // - feature map with short entry indices.

    #[test]
    fn format_1_gid_to_u8_entry_iter() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(GlyphId, u16)> = map.gid_to_entry_iter().collect();

        assert_eq!(entries, vec![(1u32.into(), 2), (2u32.into(), 1),]);
    }

    #[test]
    fn format_1_gid_to_u16_entry_iter() {
        let table = Ift::read(test_data::U16_ENTRIES_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(GlyphId, u16)> = map.gid_to_entry_iter().collect();

        assert_eq!(
            entries,
            vec![
                (2u32.into(), 0x50),
                (3u32.into(), 0x51),
                (4u32.into(), 0x12c),
                (5u32.into(), 0x12c),
                (6u32.into(), 0x50)
            ]
        );
    }

    #[test]
    fn format_1_feature_map() {
        let table = Ift::read(test_data::FEATURE_MAP_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        let Some(feature_map_result) = map.feature_map() else {
            panic!("should have a non null feature map.");
        };

        let Ok(feature_map) = feature_map_result else {
            panic!("should have a valid feature map.");
        };

        assert_eq!(feature_map.feature_records().len(), 2);

        let fr0 = feature_map.feature_records().get(0).unwrap();
        assert_eq!(fr0.feature_tag(), Tag::new(&[b'l', b'i', b'g', b'a']));
        assert_eq!(*fr0.first_new_entry_index(), U8Or16(0x70));
        assert_eq!(*fr0.entry_map_count(), U8Or16(0x02));

        let fr1 = feature_map.feature_records().get(1).unwrap();
        assert_eq!(fr1.feature_tag(), Tag::new(&[b'd', b'l', b'i', b'g']));
        assert_eq!(*fr1.first_new_entry_index(), U8Or16(0x190));
        assert_eq!(*fr1.entry_map_count(), U8Or16(0x01));
    }

    #[test]
    fn format_1_feature_entry_map() {
        let table = Ift::read(test_data::FEATURE_MAP_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        let mut it = map.entry_map_records();

        assert_eq!(
            it.next(),
            Some(FeatureEntryMapping {
                matched_entries: 0x50..=0x51,
                new_entry_index: 0x70,
                feature_tag: Tag::new(&[b'l', b'i', b'g', b'a']),
            })
        );

        assert_eq!(
            it.next(),
            Some(FeatureEntryMapping {
                matched_entries: 0x12c..=0x12c,
                new_entry_index: 0x71,
                feature_tag: Tag::new(&[b'l', b'i', b'g', b'a']),
            })
        );

        assert_eq!(
            it.next(),
            Some(FeatureEntryMapping {
                matched_entries: 0x51..=0x51,
                new_entry_index: 0x190,
                feature_tag: Tag::new(&[b'd', b'l', b'i', b'g']),
            })
        );

        assert_eq!(it.next(), None);
    }

    #[test]
    fn compatibility_id() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        assert_eq!(map.get_compatibility_id(), [1, 2, 3, 4]);
    }

    #[test]
    fn is_entry_applied() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        assert!(!map.is_entry_applied(0));
        assert!(map.is_entry_applied(1));
        assert!(!map.is_entry_applied(2));
    }

    #[test]
    fn uri_template_as_string() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        assert_eq!(Ok("ABCDEFɤ"), map.uri_template_as_string());
    }
}
