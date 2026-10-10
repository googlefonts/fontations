//! impl subset() for name table
use crate::{
    serialize::{OffsetWhence, Serializer},
    Plan, Subset,
    SubsetError::{self, SubsetTableError},
    SubsetFlags,
};

use write_fonts::{
    read::{
        tables::name::{Name, NameRecord},
        FontRef, TopLevelTable,
    },
    types::FixedSize,
    FontBuilder,
};

// reference: subset() for name table in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/OT/name/name.hh#L387
impl Subset for Name<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let data = self.offset_data().as_bytes();
        data.get(self.name_record_byte_range())
            .ok_or(SubsetTableError(Name::TAG))?;
        let mut records = Vec::new();
        let mut retained = std::collections::BTreeSet::new();
        for record in self.name_record() {
            if !plan.name_ids.contains(record.name_id())
                || !plan.name_languages.contains(record.language_id())
                || (!plan
                    .subset_flags
                    .contains(SubsetFlags::SUBSET_FLAGS_NAME_LEGACY)
                    && !record.is_unicode())
            {
                continue;
            }
            let key = (
                record.platform_id(),
                record.encoding_id(),
                record.language_id(),
                record.name_id(),
            );
            let bytes = if let Some(override_bytes) = plan.name_table_overrides.get(&key) {
                if override_bytes.is_empty() {
                    continue;
                }
                override_bytes.as_slice()
            } else if record.length() == 0 {
                &[]
            } else {
                let start =
                    self.storage_offset() as usize + record.string_offset().to_u32() as usize;
                data.get(start..start + record.length() as usize)
                    .ok_or(SubsetTableError(Name::TAG))?
            };
            records.push((key, bytes));
            retained.insert(key);
        }
        // Overrides take precedence over selection filters. Insert missing
        // nonempty records after filtering, just as HarfBuzz's name serializer.
        for (&key, bytes) in &plan.name_table_overrides {
            if !bytes.is_empty() && !retained.contains(&key) {
                records.push((key, bytes.as_slice()));
            }
        }
        records.sort_unstable_by_key(|(key, bytes)| (*key, bytes.len()));
        let count = u16::try_from(records.len()).map_err(|_| SubsetTableError(Name::TAG))?;
        let storage_offset = u16::try_from(records.len() * NAME_RECORD_SIZE + 6)
            .map_err(|_| SubsetTableError(Name::TAG))?;
        // HarfBuzz emits format 0; language-tag records are not supported yet.
        for value in [0, count, storage_offset] {
            s.embed(value).map_err(|_| SubsetTableError(Name::TAG))?;
        }
        serialize_name_records(s, &records)
    }
}

pub(crate) type RecordKey = (u16, u16, u16, write_fonts::types::NameId);

fn serialize_name_records(
    s: &mut Serializer,
    records: &[(RecordKey, &[u8])],
) -> Result<(), SubsetError> {
    for &((platform, encoding, language, id), bytes) in records {
        let offset_pos = s.length() + 10;
        for value in [
            platform,
            encoding,
            language,
            id.to_u16(),
            bytes.len() as u16,
            0,
        ] {
            s.embed(value).map_err(|_| SubsetTableError(Name::TAG))?;
        }
        if bytes.is_empty() {
            continue;
        }
        s.push().map_err(|_| SubsetTableError(Name::TAG))?;
        s.embed_bytes(bytes)
            .map_err(|_| SubsetTableError(Name::TAG))?;
        let obj_idx = s.pop_pack(true).ok_or(SubsetTableError(Name::TAG))?;
        s.add_link(
            offset_pos..offset_pos + 2,
            obj_idx,
            OffsetWhence::Tail,
            0,
            false,
        )
        .map_err(|_| SubsetTableError(Name::TAG))?;
    }
    Ok(())
}

//NameRecord size in bytes
const NAME_RECORD_SIZE: usize = NameRecord::RAW_BYTE_LEN;

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::NameId, TableProvider};
    use write_fonts::types::Tag;

    /// Returns a copy of `font_bytes` with the `count` field of the `name`
    /// table header overwritten, so that the header claims more name records
    /// than the table actually contains.
    fn font_with_bad_name_count(font_bytes: &[u8], count: u16) -> Vec<u8> {
        let mut bytes = font_bytes.to_vec();
        let font = FontRef::new(font_bytes).unwrap();
        let record = font
            .table_directory()
            .table_records()
            .iter()
            .find(|r| r.tag() == Name::TAG)
            .unwrap();
        // count is the second u16 of the name table header
        let count_pos = record.offset() as usize + 2;
        bytes[count_pos..count_pos + 2].copy_from_slice(&count.to_be_bytes());
        bytes
    }

    #[test]
    fn test_subset_name_record_count_out_of_bounds() {
        let ttf: &[u8] = include_bytes!("../test-data/fonts/Roboto-Regular.abc.ttf");
        let bytes = font_with_bad_name_count(ttf, 0xFFFF);
        let font = FontRef::new(&bytes).unwrap();
        let name = font.name().unwrap();

        let mut builder = FontBuilder::new();
        let mut plan = Plan::default();
        plan.name_ids.insert(NameId::new(1));
        plan.name_languages.insert(0x0409);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = name.subset(&plan, &font, &mut s, &mut builder);
        assert!(matches!(
            ret,
            Err(SubsetError::SubsetTableError(tag)) if tag == Name::TAG
        ));
    }

    #[test]
    fn test_subset_font_name_record_count_out_of_bounds() {
        use write_fonts::read::collections::IntSet;

        let ttf: &[u8] = include_bytes!("../test-data/fonts/Roboto-Regular.abc.ttf");
        let bytes = font_with_bad_name_count(ttf, 0xFFFF);
        let font = FontRef::new(&bytes).unwrap();

        let mut unicodes = IntSet::<u32>::empty();
        unicodes.insert_range(0x61..=0x63);
        let mut name_ids = IntSet::<NameId>::empty();
        name_ids.insert_range(NameId::from(0)..=NameId::from(6));
        let mut name_languages = IntSet::<u16>::empty();
        name_languages.insert(0x0409);
        let mut layout_features = IntSet::empty();
        layout_features.extend_unsorted(crate::DEFAULT_LAYOUT_FEATURES.iter().copied());

        let plan = Plan::new(
            &IntSet::empty(),
            &unicodes,
            &font,
            SubsetFlags::SUBSET_FLAGS_DEFAULT,
            &IntSet::empty(),
            &IntSet::<Tag>::all(),
            &layout_features,
            &name_ids,
            &name_languages,
        );

        // subset_font used to panic here. The malformed name table is now
        // reported as a subset failure, which subset() treats as "table
        // subsetted to empty", so the table is dropped from the output.
        let out = crate::subset_font(&font, &plan).unwrap();
        let subset = FontRef::new(&out).unwrap();
        assert!(subset.table_data(Name::TAG).is_none());
    }
}
