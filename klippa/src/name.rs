//! impl subset() for name table
use crate::{
    Plan,
    SubsetError::{self, SubsetTableError},
    SubsetFlags,
};

use write_fonts::{
    read::{tables::name::Name, FontRef, TableProvider, TopLevelTable},
    FontBuilder,
};

// reference: subset() for name table in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/OT/name/name.hh#L387
pub(crate) fn subset_name(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let name = font.name().or(Err(SubsetTableError(Name::TAG)))?;
    let name_records = name.name_record();
    //TODO: support name_table_override
    //TODO: support name table version 1
    let mut retained_name_record_idxes = name_records
        .iter()
        .enumerate()
        .filter_map(|(idx, record)| {
            if !plan.name_ids.contains(record.name_id())
                || !plan.name_languages.contains(record.language_id())
                || (!plan
                    .subset_flags
                    .contains(SubsetFlags::SUBSET_FLAGS_NAME_LEGACY)
                    && !record.is_unicode())
            {
                return None;
            }
            Some(idx)
        })
        .collect::<Vec<_>>();

    retained_name_record_idxes.sort_unstable_by_key(|nr| {
        let nr = name_records[*nr];
        (
            nr.platform_id(),
            nr.encoding_id(),
            nr.language_id(),
            nr.name_id().to_u16(),
            nr.length(),
        )
    });

    let name_data = name.offset_data().as_bytes();
    let mut out = Vec::with_capacity(name_data.len());
    // version
    // TODO: support version 1
    out.extend_from_slice(&[0, 0]);
    //count
    let count = retained_name_record_idxes.len() as u16;
    out.extend_from_slice(&count.to_be_bytes());
    //storage_offset
    let storage_offset = count * 12 + 6;
    out.extend_from_slice(&storage_offset.to_be_bytes());

    //pre-allocate space for name records array
    out.resize(storage_offset as usize, 0);

    let mut string_offset = 0_u16;
    let storage_start = name.storage_offset() as usize;
    for (new_idx, old_idx) in retained_name_record_idxes.iter().enumerate() {
        let old_record_start = record_start_pos(*old_idx);
        let new_record_start = record_start_pos(new_idx);
        //copy name_record except for string offset
        out.get_mut(new_record_start..new_record_start + 10)
            .unwrap()
            .copy_from_slice(
                name_data
                    .get(old_record_start..old_record_start + 10)
                    .unwrap(),
            );
        //copy string offset
        out.get_mut(new_record_start + 10..new_record_start + NAME_RECORD_SIZE)
            .unwrap()
            .copy_from_slice(&string_offset.to_be_bytes());

        //copy string data
        let str_start = storage_start + name_records[*old_idx].string_offset().to_u32() as usize;
        let str_len = name_records[*old_idx].length();
        let str_data = name_data
            .get(str_start..str_start + str_len as usize)
            .ok_or(SubsetTableError(Name::TAG))?;
        out.extend_from_slice(str_data);

        string_offset += str_len;
    }

    builder.add_raw(Name::TAG, out);
    Ok(())
}

//version + count + storageOffset field
const HEADER_SIZE: usize = 6;
//NameRecord size in bytes
const NAME_RECORD_SIZE: usize = 12;

//get the starting byte position of the ith NameRecord
fn record_start_pos(record_idx: usize) -> usize {
    HEADER_SIZE + NAME_RECORD_SIZE * record_idx
}
